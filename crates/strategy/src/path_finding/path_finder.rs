use std::{ collections::HashMap, sync::{ Arc, atomic::{ AtomicBool, Ordering } } };

use crate::{ core::strategy::Strategy, path_finding::{ Hop, balanceOfCall } };
use alloy_eips::NumHash;
use alloy_primitives::{ Address, B256, Bytes, U256 };
use alloy_rpc_types::{ AccessList, AccessListItem };
use alloy_sol_types::{ SolCall, SolValue };
use eyre::{ Error, Ok, Result };
use rayon::{ iter::{ IntoParallelRefIterator, ParallelIterator }, join };
use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth_revm::{
    Context,
    MainBuilder,
    MainContext,
    SystemCallEvm,
    context::result::{ ExecutionResult, Output, ResultAndState },
    database::StateProviderDatabase,
    db::CacheDB,
    state::Bytecode,
};
use reth_tracing::tracing;
use reth_transaction_pool::PoolTransaction;
use searcher_reth_manager::{
    common::{ CommonStrategyConfig, ONE_ETHER, StrategyConfig },
    gas::GasConfig,
    types::CandidateEntry,
};

use crate::path_finding::types::executeCall;

use super::types::getProfitCall;
use crate::profit_reporter::record_profit;
use serde_json;

const PROFITABLE_PATHS_LIMIT: usize = 10;
const MAX_SEARCH_DEPTH: usize = 20;
const INV_GOLDEN_RATIO_NUM: u128 = 618_033_988_749_894_848;
const INV_GOLDEN_RATIO_DEN: u128 = 1_000_000_000_000_000_000;

pub struct PathFinder {
    config: StrategyConfig,
    code: Bytecode,

    // decoded routes
    candidates: Vec<CandidateEntry>,
}

impl Strategy for PathFinder {
    type Action = Hop;

    fn new(config: StrategyConfig) -> Self {
        let code = config.get_contract();
        Self { config: config.clone(), code, candidates: Vec::new() }
    }

    fn gas_config(&self) -> GasConfig {
        self.config.get_gas_config()
    }

    fn get_code(&self) -> Bytecode {
        self.code.clone()
    }

    fn get_vault(&self) -> Address {
        self.config.get_vault()
    }

    fn prepare(&mut self, chain_id: u64) {
        tracing::info!(
            target: "path-finder",
            event = "prepare_enter",
            chain_id = chain_id,
            "Entered prepare()"
        );
        let candidates = self.config.load_candidates(chain_id);
        tracing::info!(
            target: "path-finder",
            event = "candidates_count",
            chain_id = chain_id,
            count = candidates.len(),
            "Prepare candidates for path finding",
        );
        self.candidates = candidates;
        tracing::info!(
            target: "path-finder",
            event = "prepare_exit",
            chain_id = chain_id,
            "Exiting prepare()"
        );
    }

    /// Finds profitable candidates from the pending transactions and candidates.
    fn find_profitable_candidates<
        T: PoolTransaction,
        DB: DBProvider + BlockHashReader + StateCommitmentProvider
    >(
        &mut self,
        block: NumHash,
        latest_state_provider: LatestStateProviderRef<'_, DB>,
        pending_txs: Vec<T>
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error> {
        // 0. Check if the vault address is zero, if so, skip to make calldata
        let vault = self.get_vault();
        let no_vault = vault == Address::ZERO;

        let balance_map = if no_vault {
            tracing::warn!(
                target: "path-finder",
                event = "no_vault",
                "Vault address is zero, skipping candidate"
            );
            HashMap::new()
        } else {
            Self::get_balances(&latest_state_provider, vault, &self.candidates)
        };

        // 1. Get dirty states from pending transactions
        let dirty_states = Self::collect_dirty_states_from_pending_txs(
            pending_txs,
            &latest_state_provider
        );

        // 2. Filter candidates based on liquidity and profit ranges
        let (max_profit, min_profit) = self.config.get_profit_range();
        let (min_liquidity, max_liquidity) = self.config.get_liquidity_range();

        tracing::info!(
            target: "path-finder",
            event = "search_ranges",
            min_liquidity = %min_liquidity,
            max_liquidity = %max_liquidity,
            min_profit = %min_profit,
            max_profit = %max_profit,
            "Starting search with configured ranges",
        );

        let candidates = self.candidates.clone();

        let found_max_profit = Arc::new(AtomicBool::new(false));
        let result = candidates
            .par_iter()
            .take_any_while(|_| !found_max_profit.load(Ordering::Relaxed))
            .take_any(PROFITABLE_PATHS_LIMIT)
            .fold(
                || {
                    (
                        Vec::<U256>::new(), // amounts
                        Vec::<Bytes>::new(), // executor calldata
                        HashMap::<Address, Vec<B256>>::new(), // access lists
                    )
                },
                |mut acc, candidate| {
                    let CandidateEntry { initial_token, encoded: encoded_calldata } = candidate;
                    if found_max_profit.load(Ordering::Relaxed) {
                        return acc;
                    }

                    let effective_max_liquidity = if no_vault {
                        max_liquidity
                    } else {
                        match balance_map.get(initial_token) {
                            Some(balance) => *balance,
                            None => {
                                return acc;
                            }
                        }
                    };

                    let (golden_input, golden_output) = match
                        self.golden_section_search(
                            &latest_state_provider,
                            min_liquidity,
                            effective_max_liquidity,
                            encoded_calldata
                        )
                    {
                        Some(res) if !found_max_profit.load(Ordering::Relaxed) => res,
                        _ => {
                            return acc;
                        }
                    };

                    if golden_input > golden_output {
                        return acc;
                    }

                    let profit = golden_output - golden_input;

                    let route_info = match <Vec<Hop>>::abi_decode(encoded_calldata) {
                        std::result::Result::Ok(hops) => format!("Route: {:?}", hops),
                        Err(_) => {
                            format!("Failed to Decode Route: Raw bytes: {:?}", encoded_calldata)
                        }
                    };

                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        amount = %golden_input,
                        profit = %profit.div_ceil(U256::from(ONE_ETHER)),
                        calldata = route_info,
                        "get optimal amount and profit from golden section search",
                    );

                    if profit.ge(&max_profit) {
                        found_max_profit.store(true, Ordering::Relaxed);
                    } else if !profit.ge(&min_profit) {
                        tracing::info!(
                            target: "path-finder",
                            event = "profit",
                            sub_event = "filtered",
                            filter = "profit range",
                            profit = %profit.div_ceil(U256::from(ONE_ETHER)),
                            calldata = route_info,
                            "filter route profit below minimum threshold",
                        );
                        return acc;
                    }

                    let profit_info =
                        serde_json::json!({
                        "block": block.number,
                        "amount": golden_input.div_ceil(U256::from(ONE_ETHER)).to_string(),
                        "profit": profit.div_ceil(U256::from(ONE_ETHER)).to_string(),
                        "route": route_info,
                    });
                    record_profit(profit_info);

                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        profit =  %profit.div_ceil(U256::from(ONE_ETHER)),
                        min_profit = %min_profit,
                        route = %route_info,
                        vault = no_vault,
                        "filtered by profit",
                    );

                    if no_vault {
                        return acc;
                    }

                    let ResultAndState { result: _, state } = match
                        self.call_execute(
                            &latest_state_provider,
                            (executeCall {
                                amounts: vec![golden_input],
                                calldata: vec![encoded_calldata.clone().into()],
                            }).abi_encode()
                        )
                    {
                        std::result::Result::Ok(res) => res,
                        Err(e) => {
                            tracing::warn!(
                                target: "reth-exex",
                                event = "execute failed",
                                error = ?e,
                            );
                            return acc;
                        }
                    };
                    let clean_states = match Self::collect_clean_states(&state, &dirty_states) {
                        Some(cs) => cs,
                        None => {
                            return acc;
                        }
                    };

                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        filter = "dirty states",
                        profit = %golden_output,
                        route = ?encoded_calldata,
                        "filtered by dirty states",
                    );

                    acc.0.push(golden_input);
                    acc.1.push(Bytes::from(encoded_calldata.clone()));
                    for item in clean_states {
                        if let Some(storage_keys) = acc.2.get_mut(&item.address) {
                            storage_keys.extend(item.storage_keys);
                        } else {
                            acc.2.insert(item.address, item.storage_keys);
                        }
                    }

                    acc
                }
            )
            // Combine results from all threads
            .reduce_with(|mut acc, curr| {
                acc.0.extend(curr.0);
                acc.1.extend(curr.1);
                for (address, storage_keys) in curr.2 {
                    if let Some(existing_keys) = acc.2.get_mut(&address) {
                        existing_keys.extend(storage_keys);
                    } else {
                        acc.2.insert(address, storage_keys);
                    }
                }

                acc
            })
            // Convert to the final result
            .map(|(amounts, routes, access_map)| {
                let access_list = AccessList::from(
                    access_map
                        .into_iter()
                        .map(|(address, storage_keys)| AccessListItem { address, storage_keys })
                        .collect::<Vec<AccessListItem>>()
                );

                if !routes.is_empty() {
                    tracing::info!(
                        target: "reth-exex",
                        event = "send_profitable_candidates_to_relayer_pool",
                        route_len = routes.len(),
                        routes = ?routes,
                    );
                }

                let calldata = (executeCall { amounts, calldata: routes }).abi_encode();
                (calldata, access_list)
            });

        Ok(result)
    }
}

impl PathFinder {
    fn get_balances<DB: DBProvider + BlockHashReader + StateCommitmentProvider>(
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
        vault: Address,
        candidates: &[CandidateEntry]
    ) -> HashMap<Address, U256> {
        let db = CacheDB::new(StateProviderDatabase::new(latest_state_provider));
        let mut evm = Context::mainnet().with_db(db).build_mainnet();
        let mut balances = HashMap::new();
        let balance_call = (balanceOfCall { account: vault }).abi_encode();
        for candidate in candidates {
            let CandidateEntry { initial_token, encoded: _ } = candidate;
            match evm.transact_system_call(*initial_token, balance_call.clone().into()) {
                std::result::Result::Ok(result) => {
                    if let Some(output_bytes) = result.output() {
                        if let std::result::Result::Ok(balance) = U256::abi_decode(output_bytes) {
                            balances.insert(*initial_token, balance);
                        } else {
                            tracing::warn!(
                                target: "reth-exex",
                                event = "get_balance_decode_failed",
                                call = ?balance_call,
                            );
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        target: "reth-exex",
                        event = "get_balance_call_failed",
                        error = ?err,
                        call = ?balance_call,
                    );
                }
            }
        }
        balances
    }

    fn golden_section_search<DB: DBProvider + BlockHashReader + StateCommitmentProvider>(
        &self,
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
        min_liquidity: U256,
        max_liquidity: U256,
        candidate: &[u8]
    ) -> Option<(U256, U256)> {
        let mut left = min_liquidity;
        let mut right = max_liquidity;

        let diff = right - left;
        let mut mid1 =
            right - (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
        let mut mid2 =
            left + (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);

        let (mut mid1_output, mut mid2_output) = join(
            || self.get_profit(latest_state_provider, mid1, candidate),
            || self.get_profit(latest_state_provider, mid2, candidate)
        );

        if mid1_output.is_none() || mid2_output.is_none() {
            return None;
        }
        for _ in 0..MAX_SEARCH_DEPTH {
            if right <= left + U256::from(1u8) {
                break;
            }

            if mid1_output < mid2_output {
                left = mid1;
                mid1 = mid2;
                mid1_output = mid2_output;

                let diff = right - left;
                mid2 =
                    left +
                    (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
                mid2_output = self.get_profit(latest_state_provider, mid2, candidate);
                mid2_output?;
            } else {
                right = mid2;
                mid2 = mid1;
                mid2_output = mid1_output;

                let diff = right - left;
                mid1 =
                    right -
                    (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
                mid1_output = self.get_profit(latest_state_provider, mid1, candidate);
                mid1_output?;
            }
        }

        if mid1_output.unwrap() >= mid2_output.unwrap() {
            Some((mid1, mid1_output.unwrap()))
        } else {
            Some((mid2, mid2_output.unwrap()))
        }
    }

    fn get_profit<DB>(
        &self,
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
        amount: U256,
        encoded_calldata: &[u8]
    ) -> Option<U256>
        where DB: DBProvider + BlockHashReader + StateCommitmentProvider
    {
        let profit_call = getProfitCall { amount, calldata: encoded_calldata.to_owned().into() };
        let encoded = profit_call.abi_encode();
        let result = self.call_get_profit(latest_state_provider, encoded.clone());
        if result.is_err() {
            tracing::warn!(
                target: "reth-exex",
                event = "get_profit_call_failed",
                error = ?result.err(),
                call = ?profit_call,
            );
            return None;
        }
        match result.unwrap() {
            ExecutionResult::Success { output: Output::Call(value), .. } => {
                tracing::info!(
                    target: "reth-exex",
                    event = "get_profit_call_success",
                    call = ?profit_call,
                    value = ?value,
                );
                Some(U256::abi_decode(&value).unwrap_or_default())
            }
            ExecutionResult::Revert { output, .. } => {
                tracing::warn!(
                    target: "reth-exex",
                    event = "get_profit_call_reverted",
                    output = ?output,
                    call = ?profit_call,
                );
                None
            }
            ExecutionResult::Halt { reason, .. } => {
                tracing::warn!(
                    target: "reth-exex",
                    event = "get_profit_call_failed",
                    error = ?reason,
                    call = ?profit_call,
                );
                None
            }
            _ => None,
        }
    }
}
