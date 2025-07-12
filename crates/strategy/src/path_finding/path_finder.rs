use std::{ collections::HashMap, sync::{ Arc, atomic::{ AtomicBool, Ordering } } };

use crate::core::strategy::Strategy;
use alloy_primitives::{ Address, B256, U256 };
use alloy_rpc_types::{ AccessList, AccessListItem };
use alloy_sol_types::{ SolCall, SolValue };
use eyre::{ Error, Ok, Result };
use rayon::{ iter::{ IntoParallelRefIterator, ParallelIterator }, join };
use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth_revm::{
    context::{ result::{ ExecutionResult, Output, ResultAndState }, TxEnv },
    database::StateProviderDatabase,
    db::CacheDB,
    state::Bytecode,
    Context,
    Database,
    MainBuilder,
    MainContext,
};
use reth_tracing::tracing;
use reth_transaction_pool::PoolTransaction;
use searcher_reth_manager::{
    common::{ CommonStrategyConfig, StrategyConfig },
    gas::GasConfig,
    types::Candidate,
};

use crate::path_finding::types::executeCall;

use super::types::{ Hop, getProfitCall };

const PROFITABLE_PATHS_LIMIT: usize = 10;
const MAX_SEARCH_DEPTH: usize = 20;
const INV_GOLDEN_RATIO_NUM: u128 = 618_033_988_749_894_848;
const INV_GOLDEN_RATIO_DEN: u128 = 1_000_000_000_000_000_000;

pub struct PathFinder {
    config: StrategyConfig,
    candidates: Option<Vec<Candidate>>,
    code: Bytecode,
}

impl Strategy for PathFinder {
    type Action = Hop;

    fn new(config: StrategyConfig) -> Self {
        let code = config.get_contract();

        Self { config: config.clone(), candidates: None, code }
    }

    fn gas_config(&self) -> GasConfig {
        self.config.get_gas_config()
    }

    fn get_or_load_candidates(&mut self, chain_id: u64) -> Vec<Candidate> {
        if let Some(candidates) = &self.candidates {
            return candidates.clone();
        }

        let candidates = self.config.load_candidates(chain_id);
        self.candidates = Some(candidates.clone());
        candidates
    }

    fn get_code(&self) -> Bytecode {
        self.code.clone()
    }

    fn get_vault(&self) -> Address {
        self.config.get_vault()
    }

    fn find_profitable_candidates<
        T: PoolTransaction,
        DB: DBProvider + BlockHashReader + StateCommitmentProvider
    >(
        &mut self,
        latest_state_provider: LatestStateProviderRef<'_, DB>,
        pending_txs: Vec<T>,
        candidates: Vec<Candidate>
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error> {
        // 0. Check if the vault address is zero, if so, skip to make calldata
        let no_vault = self.get_vault() == Address::ZERO;
        if no_vault {
            tracing::warn!(
                target: "path-finder",
                event = "no_vault",
                "Vault address is zero, skipping candidate"
            );
        }

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
            "starting search with configured ranges",
        );

        let found_max_profit = Arc::new(AtomicBool::new(false));
        let result = candidates
            .par_iter()
            .take_any_while(|_| !found_max_profit.load(Ordering::Relaxed))
            .take_any(PROFITABLE_PATHS_LIMIT)
            .fold(
                || {
                    (
                        Vec::<U256>::new(), // amounts
                        Vec::<Vec<Hop>>::new(), // routes
                        HashMap::<Address, Vec<B256>>::new(), // access lists
                    )
                },
                |mut acc, candidate: &Candidate| {
                    if found_max_profit.load(Ordering::Relaxed) {
                        return acc;
                    }
                    // 2-1. Decode hops
                    let mut route = Vec::new();
                    for hop in candidate.iter() {
                        match Hop::abi_decode(hop) {
                            std::result::Result::Ok(decoded_hop) => {
                                route.push(decoded_hop);
                            }
                            Err(_) => {
                                tracing::warn!(
                                    target: "reth-exex",
                                    event = "hop_decode_failed",
                                    data = ?hop,
                                    "Failed to decode hop in candidate, skipping entire candidate"
                                );
                                return acc;
                            }
                        }
                    }

                    // 2-2. Search optimal amount in liqudity range to get maximum profit
                    let (amount, net_profit) = match
                        self.golden_section_search(
                            &latest_state_provider,
                            min_liquidity,
                            max_liquidity,
                            &route
                        )
                    {
                        Some(res) if !found_max_profit.load(Ordering::Relaxed) => res,
                        _ => {
                            return acc;
                        }
                    };

                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        amount = %amount,
                        profit = %net_profit,
                        route = ?route,
                        "get optimal amount and profit from golden section search",
                    );

                    // 2-3. Filter out based on profit range
                    let route = if net_profit.ge(&max_profit) {
                        found_max_profit.store(true, Ordering::Relaxed);
                        route.clone()
                    } else if net_profit.ge(&min_profit) {
                        route.clone()
                    } else {
                        tracing::info!(
                            target: "path-finder",
                            event = "profit",
                            sub_event = "filtered",
                            filter = "profit range",
                            profit = %net_profit,
                            route = ?route.clone(),
                            "filter route profit below minimum threshold",
                        );
                        return acc;
                    };

                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        profit = %net_profit,
                        min_profit = %min_profit,
                        route = ?route.clone(),
                        vault = no_vault,
                        "filtered by profit",
                    );

                    if no_vault {
                        return acc;
                    }
                    // 2-4. Execute execute function of vault to get access list and to check it has
                    // dirty states, it it have, filter them out
                    let ResultAndState { result: _, state } = match
                        self.call_execute(
                            &latest_state_provider,
                            (executeCall {
                                amounts: vec![amount],
                                routes: vec![route.clone()],
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
                        profit = %net_profit,
                        route = ?route.clone(),
                        "filtered by dirty states",
                    );

                    // 2-5. accumulate result
                    acc.0.push(amount);
                    acc.1.push(route);
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

                let calldata = (executeCall { amounts, routes }).abi_encode();
                (calldata, access_list)
            });

        Ok(result)
    }
}

impl PathFinder {
    fn get_profit<DB>(
        &self,
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
        amount: U256,
        route: Vec<Hop>
    ) -> Option<U256>
        where DB: DBProvider + BlockHashReader + StateCommitmentProvider
    {
        {
            // validate the dex have code
            let mut db = CacheDB::new(StateProviderDatabase::new(latest_state_provider));
            let res = db.basic(route.first()?.dex);
            match res {
                std::result::Result::Ok(Some(account)) => {
                    tracing::warn!(
                        target: "reth-exex",
                        event = "get_profit_call_failed",
                        error = "Dex contract has code",
                        dex = ?account,
                    );
                    let code_hash = account.code_hash;

                    let ret = db.code_by_hash(code_hash);
                    if ret.is_err() {
                        tracing::warn!(
                            target: "reth-exex",
                            event = "get_profit_call_failed",
                            error = "Dex contract code not found",
                            dex = ?route.first()?.dex,
                        );
                        return None;
                    }
                    let bytecode = ret.unwrap();

                    tracing::warn!(
                            target: "reth-exex",
                            event = "get_profit_call_failed",
                            error = "Dex contract code exists",
                            dex = ?bytecode,
                        );
                    return None;
                }
                std::result::Result::Ok(None) => {
                    tracing::warn!(
                        target: "reth-exex",
                        event = "get_profit_call_failed",
                        error = "Dex account does not exist",
                        dex = ?route.first()?.dex,
                    );
                    return None;
                }
                Err(e) => {
                    tracing::warn!(
                        target: "reth-exex",
                        event = "get_profit_call_failed",
                        error = ?e,
                    );
                    return None;
                }
                _ => {}
            }
        }
        let encoded = (getProfitCall { amount, route }).abi_encode();
        let result = self.call_get_profit(latest_state_provider, encoded);
        if result.is_err() {
            tracing::warn!(
                target: "reth-exex",
                event = "get_profit_call_failed",
                error = ?result.err(),
            );
            return None;
        }
        match result.unwrap() {
            ExecutionResult::Success { output: Output::Call(value), .. } => {
                Some(U256::abi_decode(&value).unwrap_or_default())
            }
            ExecutionResult::Revert { output, .. } => {
                tracing::warn!(
                    target: "reth-exex",
                    event = "get_profit_call_reverted",
                    output = ?output,
                );
                None
            }
            ExecutionResult::Halt { reason, .. } => {
                tracing::warn!(
                    target: "reth-exex",
                    event = "get_profit_call_failed",
                    error = ?reason,
                );
                None
            }
            _ => None,
        }
    }

    fn golden_section_search<DB: DBProvider + BlockHashReader + StateCommitmentProvider>(
        &self,
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
        min_liquidity: U256,
        max_liquidity: U256,
        hops: &[Hop]
    ) -> Option<(U256, U256)> {
        let mut left = min_liquidity;
        let mut right = max_liquidity;

        let diff = right - left;
        let mut mid1 =
            right - (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
        let mut mid2 =
            left + (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);

        let (mut profit1, mut profit2) = join(
            || self.get_profit(latest_state_provider, mid1, hops.to_owned()),
            || self.get_profit(latest_state_provider, mid2, hops.to_owned())
        );

        if profit1.is_none() || profit2.is_none() {
            return None;
        }
        for _ in 0..MAX_SEARCH_DEPTH {
            if right <= left + U256::from(1u8) {
                break;
            }

            if profit1 < profit2 {
                left = mid1;
                mid1 = mid2;
                profit1 = profit2;

                let diff = right - left;
                mid2 =
                    left +
                    (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
                profit2 = self.get_profit(latest_state_provider, mid2, hops.to_owned());
                profit2?;
            } else {
                right = mid2;
                mid2 = mid1;
                profit2 = profit1;

                let diff = right - left;
                mid1 =
                    right -
                    (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
                profit1 = self.get_profit(latest_state_provider, mid1, hops.to_owned());
                profit1?;
            }
        }

        if profit1.unwrap() >= profit2.unwrap() {
            Some((mid1, profit1.unwrap()))
        } else {
            Some((mid2, profit2.unwrap()))
        }
    }
}
