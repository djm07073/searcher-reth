use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use alloy_primitives::{Address, B256, U256, map::HashSet};
use alloy_rpc_types::{AccessList, AccessListItem};
use alloy_sol_types::{SolCall, SolValue};
use eyre::{Error, Ok, Result};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    join,
};
use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    Context, MainBuilder, MainContext, SystemCallEvm,
    context::{
        BlockEnv, CfgEnv, Evm, TxEnv,
        result::{ExecutionResult, Output, ResultAndState},
    },
    database::StateProviderDatabase,
    db::CacheDB,
    handler::{EthPrecompiles, instructions::EthInstructions},
    interpreter::interpreter::EthInterpreter,
    state::{AccountInfo, Bytecode},
};
use reth_tracing::tracing;
use reth_transaction_pool::PoolTransaction;
use searcher_reth_config::{
    strategy::{CommonStrategyConfig, StrategyConfig},
    types::Candidate,
};
use searcher_reth_core::strategy::{STRATEGY_CONTRACT_ADDRESS, Strategy};

use crate::path_finding::types::executeCall;

use super::types::{Hop, getProfitCall};

type PathFinderCtx<'a, DB> = Context<
    BlockEnv,
    TxEnv,
    CfgEnv,
    CacheDB<StateProviderDatabase<LatestStateProviderRef<'a, DB>>>,
>;

type PathFinderEvm<'a, DB> = Evm<
    PathFinderCtx<'a, DB>,
    (),
    EthInstructions<EthInterpreter, PathFinderCtx<'a, DB>>,
    EthPrecompiles,
>;

const PROFITABLE_PATHS_LIMIT: usize = 10;
const MAX_SEARCH_DEPTH: usize = 20;

pub struct PathFinder<'a, StrategyDatabase>
where
    StrategyDatabase: DBProvider + BlockHashReader + StateCommitmentProvider,
{
    evm: Option<PathFinderEvm<'a, StrategyDatabase>>,
    contract: Bytecode,
    liquidity_range: (U256, U256),
    profit_ratio_range: (U256, U256),
}

impl<'a, StrategyDatabase> Strategy<'a> for PathFinder<'a, StrategyDatabase>
where
    StrategyDatabase: DBProvider + BlockHashReader + StateCommitmentProvider,
{
    type Action = Hop;

    type DB = StrategyDatabase;

    fn new(config: &StrategyConfig) -> Self {
        let contract = config.get_contract();
        let (max_profit_ratio, min_profit_ratio) = config.get_profit_ratios();
        let (max_liquidity, min_liquidity) = config.get_liquidity_range();
        Self {
            evm: None,
            contract,
            liquidity_range: (min_liquidity, max_liquidity),
            profit_ratio_range: (min_profit_ratio, max_profit_ratio),
        }
    }

    fn set_last_state(&mut self, provider: LatestStateProviderRef<'a, Self::DB>) {
        let mut db: CacheDB<StateProviderDatabase<LatestStateProviderRef<'a, StrategyDatabase>>> =
            CacheDB::new(StateProviderDatabase::new(provider));
        let contract = self.contract.clone();
        db.insert_account_info(
            STRATEGY_CONTRACT_ADDRESS,
            AccountInfo {
                code_hash: contract.hash_slow(),
                code: Some(contract.clone()),
                ..Default::default()
            },
        );

        let evm = Context::mainnet().with_db(db).build_mainnet();

        self.evm = Some(evm);
    }

    fn get_code(&self) -> Bytecode {
        self.contract.clone()
    }

    fn find_profitable_candidates<T: PoolTransaction>(
        &mut self,
        pending_txs: Vec<T>,
        candidates: Vec<Candidate>,
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error> {
        // 1. Get dirty states from pending transactions
        let evm = self.evm.as_mut().unwrap();
        let pevm = Arc::new(Mutex::new(evm));
        let dirty_states = pending_txs
            .par_iter()
            .filter_map(|tx| {
                let to = tx.to()?;
                let data = tx.input().clone();
                let result = pevm.lock().unwrap().transact_system_call(data, to).unwrap();
                let dirty_state: HashMap<Address, HashSet<U256>> = result
                    .state
                    .iter()
                    .filter(|(_, account)| account.is_touched())
                    .fold(HashMap::new(), |mut acc, (address, account)| {
                        let changed_storage_keys: HashSet<U256> = account
                            .storage
                            .iter()
                            .filter(|(_, storage_slot)| storage_slot.is_changed())
                            .map(|(key, _)| *key)
                            .collect();

                        if !changed_storage_keys.is_empty() {
                            acc.entry(*address).or_default().extend(changed_storage_keys);
                        }
                        acc
                    });
                Some(dirty_state)
            })
            .reduce_with(|mut acc, dirty_state| {
                for (address, keys) in dirty_state {
                    acc.entry(address).or_default().extend(keys);
                }
                acc
            })
            .unwrap_or_default();

        // 2. Filter candidates based on vault balances and profit ratios
        let (max_profit_ratio, min_profit_ratio) = self.profit_ratio_range;
        let (min_liquidity, max_liquidity) = self.liquidity_range;
        let found_max_profit = Arc::new(AtomicBool::new(false));

        let result = candidates
            .par_iter()
            .take_any_while(|_| !found_max_profit.load(Ordering::Relaxed))
            .take_any(PROFITABLE_PATHS_LIMIT)
            .fold(
                || {
                    (
                        Vec::<U256>::new(),                   // amounts
                        Vec::<Vec<Hop>>::new(),               // routes
                        HashMap::<Address, Vec<B256>>::new(), // access lists
                    )
                },
                |mut acc, candidate: &Candidate| {
                    if found_max_profit.load(Ordering::Relaxed) {
                        return acc;
                    }
                    // Decode candidate hops
                    let mut hops = Vec::new();
                    for hop in candidate.iter() {
                        match Hop::abi_decode(hop) {
                            std::result::Result::Ok(decoded_hop) => {
                                hops.push(decoded_hop);
                            }
                            Err(_) => {
                                tracing::warn!(
                                    target: "reth-exex",
                                    action = "hop_decode_failed",
                                    data = ?hop,
                                    "Failed to decode hop in candidate, skipping entire candidate"
                                );
                                return acc;
                            }
                        }
                    }

                    let mut left = min_liquidity;
                    let mut right = max_liquidity;

                    for _ in 0..MAX_SEARCH_DEPTH {
                        if right <= left + U256::from(1u8) {
                            break;
                        }

                        let diff = right - left;
                        let one_third = diff / U256::from(3u8);
                        let mid1 = left + one_third;
                        let mid2 = right - one_third;

                        let (profit1, profit2) = join(
                            || Self::call_get_profit(&pevm, mid1, hops.clone()),
                            || Self::call_get_profit(&pevm, mid2, hops.clone()),
                        );

                        if profit1 < profit2 {
                            left = mid1;
                        } else {
                            right = mid2;
                        }
                    }

                    let (profit_left, profit_right) = join(
                        || Self::call_get_profit(&pevm, left, hops.clone()),
                        || Self::call_get_profit(&pevm, right, hops.clone()),
                    );

                    let amount = if profit_left >= profit_right { left } else { right };
                    if found_max_profit.load(Ordering::Relaxed) {
                        return acc;
                    }

                    let encoded_data = (getProfitCall { amount, route: hops.clone() }).abi_encode();

                    let element: Option<(Vec<Hop>, Vec<AccessListItem>)> = {
                        let mut evm = pevm.lock().unwrap();
                        // TODO: access list from execute contract not from simulation contract
                        let ResultAndState { result, state } = evm
                            .transact_system_call(encoded_data.into(), STRATEGY_CONTRACT_ADDRESS)
                            .unwrap();

                        let clean_states = Self::collect_clean_states(&state, &dirty_states);

                        let net_profit = match result {
                            ExecutionResult::Success { output: Output::Call(value), .. } => {
                                <U256>::abi_decode(&value).unwrap()
                            }

                            ExecutionResult::Revert { gas_used: _, output } => {
                                tracing::error!(
                                    target: "reth-exex",
                                    action = "get_profit_call_revert",
                                    "getProfitCall reverted with output: {:?}",
                                    output
                                );
                                return acc;
                            }
                            _ => {
                                return acc;
                            }
                        };

                        let net_profit_ratio = net_profit.checked_div(amount).unwrap();

                        if net_profit_ratio.ge(&max_profit_ratio) {
                            found_max_profit.store(true, Ordering::Relaxed);
                            Some((hops.clone(), clean_states.unwrap()))
                        } else if net_profit_ratio.ge(&min_profit_ratio) {
                            Some((hops.clone(), clean_states.unwrap()))
                        } else {
                            return acc;
                        }
                    };

                    if element.is_none() {
                        return acc;
                    }
                    let (profitable_paths, access_items) = element.unwrap();

                    // accumulate results
                    acc.0.push(amount);
                    acc.1.push(profitable_paths);
                    for item in access_items {
                        if let Some(storage_keys) = acc.2.get_mut(&item.address) {
                            storage_keys.extend(item.storage_keys);
                        } else {
                            acc.2.insert(item.address, item.storage_keys);
                        }
                    }

                    acc
                },
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
                        .collect::<Vec<AccessListItem>>(),
                );

                if !routes.is_empty() {
                    // Log the routes being sent
                    let routes = routes
                        .clone()
                        .iter()
                        .map(|route| format!("{:?}", route))
                        .collect::<Vec<String>>();
                    let route_len = routes.len();
                    tracing::info!(
                        target: "reth-exex",
                        action = "send_profitable_candidates_to_relayer_pool",
                        route_len = route_len,
                        routes = routes.join(", "),
                    );
                }
                let calldata = (executeCall { amounts, routes }).abi_encode();

                (calldata, access_list)
            });

        Ok(result)
    }
}

impl<'a, StrategyDatabase> PathFinder<'a, StrategyDatabase>
where
    StrategyDatabase: DBProvider + BlockHashReader + StateCommitmentProvider,
{
    fn call_get_profit(
        pevm: &Arc<Mutex<&mut PathFinderEvm<'a, StrategyDatabase>>>,
        amount: U256,
        route: Vec<Hop>,
    ) -> U256 {
        let encoded = (getProfitCall { amount, route }).abi_encode();
        let mut evm = pevm.lock().unwrap();
        let ResultAndState { result, .. } =
            evm.transact_system_call(encoded.into(), STRATEGY_CONTRACT_ADDRESS).unwrap();
        match result {
            ExecutionResult::Success { output: Output::Call(value), .. } => {
                <U256>::abi_decode(&value).unwrap_or_default()
            }
            _ => U256::ZERO,
        }
    }
}
