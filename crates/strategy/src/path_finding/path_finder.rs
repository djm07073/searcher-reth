use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    core::strategy::Strategy,
    path_finding::{Hop, RouterHop},
};
use alloy_eips::NumHash;
use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::{AccessList, AccessListItem};
use alloy_sol_types::{SolCall, SolValue};
use eyre::{Error, Ok, Result};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    join,
};
use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    context::result::{ExecutionResult, Output, ResultAndState},
    state::Bytecode,
};
use reth_tracing::tracing;
use reth_transaction_pool::PoolTransaction;
use searcher_reth_manager::{
    common::{CommonStrategyConfig, ONE_ETHER, StrategyConfig},
    gas::GasConfig,
};

use crate::path_finding::types::executeCall;

use super::types::{QuoterHop, getProfitCall};
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
    routes: Vec<(Vec<QuoterHop>, Vec<RouterHop>)>,
}

impl Strategy for PathFinder {
    type Action = QuoterHop;

    fn new(config: StrategyConfig) -> Self {
        let code = config.get_contract();
        Self { config: config.clone(), code, routes: Vec::new() }
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

    /// Load candidates and prepare routes
    fn prepare(&mut self, chain_id: u64) {
        let candidates = self.config.load_candidates(chain_id);
        let mut routes = Vec::new();
        for candidate in candidates.iter() {
            for hop in candidate.iter() {
                let mut quoter_route = Vec::new();
                let mut router_route = Vec::new();
                match Hop::abi_decode(hop) {
                    std::result::Result::Ok(decoded_hop) => {
                        quoter_route.push(decoded_hop.clone().into());
                        router_route.push(decoded_hop.into());
                    }
                    Err(_) => {
                        tracing::warn!(
                            target: "reth-exex",
                            event = "hop_decode_failed",
                            data = ?hop,
                            "Failed to decode hop in candidate, skipping entire candidate"
                        );
                    }
                }
                routes.push((quoter_route, router_route));
            }
        }
        self.routes = routes;
    }

    /// Finds profitable candidates from the pending transactions and candidates.
    fn find_profitable_candidates<
        T: PoolTransaction,
        DB: DBProvider + BlockHashReader + StateCommitmentProvider,
    >(
        &mut self,
        block: NumHash,
        latest_state_provider: LatestStateProviderRef<'_, DB>,
        pending_txs: Vec<T>,
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
        let dirty_states =
            Self::collect_dirty_states_from_pending_txs(pending_txs, &latest_state_provider);

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

        let routes = self.routes.clone();
        let found_max_profit = Arc::new(AtomicBool::new(false));
        let result = routes
            .par_iter()
            .take_any_while(|_| !found_max_profit.load(Ordering::Relaxed))
            .take_any(PROFITABLE_PATHS_LIMIT)
            .fold(
                || {
                    (
                        Vec::<U256>::new(), // amounts
                        Vec::<Vec<RouterHop>>::new(), // routes
                        HashMap::<Address, Vec<B256>>::new(), // access lists
                    )
                },
                |mut acc, (quoter_route, router_route)| {
                    if found_max_profit.load(Ordering::Relaxed) {
                        return acc;
                    }
                    // 2-1. Search optimal amount in liqudity range to get maximum profit
                    let (optimal_input, optimal_output) = match
                        self.golden_section_search(
                            &latest_state_provider,
                            min_liquidity,
                            max_liquidity,
                            quoter_route
                        )
                    {
                        Some(res) if !found_max_profit.load(Ordering::Relaxed) => res,
                        _ => {
                            return acc;
                        }
                    };

                    if optimal_input < optimal_output {
                        return acc;
                    }

                    let profit = optimal_output - optimal_input;
                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        amount = %optimal_input,
                        profit = %profit.div_ceil(U256::from(ONE_ETHER)),
                        route = ?quoter_route,
                        "get optimal amount and profit from golden section search",
                    );

                    // 2-2. Filter out based on profit range
                    let quoter_route = if profit.ge(&max_profit) {
                        found_max_profit.store(true, Ordering::Relaxed);
                        quoter_route.clone()
                    } else if profit.ge(&min_profit) {
                        quoter_route.clone()
                    } else {
                        tracing::info!(
                            target: "path-finder",
                            event = "profit",
                            sub_event = "filtered",
                            filter = "profit range",
                            profit = %profit.div_ceil(U256::from(ONE_ETHER)),
                            route = ?quoter_route.clone(),
                            "filter route profit below minimum threshold",
                        );
                        return acc;
                    };
                    let profit_info =
                        serde_json::json!({
                        "block": block.number,
                        "token": quoter_route[0].srcToken.to_string(),
                        "amount": optimal_input.div_ceil(U256::from(ONE_ETHER)).to_string(),
                        "profit": profit.div_ceil(U256::from(ONE_ETHER)).to_string(),
                        "route": quoter_route.iter().map(|hop| format!("{:?}", hop)).collect::<Vec<_>>(),
                    });
                    record_profit(profit_info);

                    tracing::info!(
                        target: "path-finder",
                        event = "profit",
                        sub_event = "predicted",
                        profit =  %profit.div_ceil(U256::from(ONE_ETHER)),
                        min_profit = %min_profit,
                        route = ?quoter_route.clone(),
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
                                amounts: vec![optimal_input],
                                routes: vec![router_route.clone()],
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
                        profit = %optimal_output,
                        route = ?router_route.clone(),
                        "filtered by dirty states",
                    );

                    // 2-5. accumulate result
                    acc.0.push(optimal_input);
                    acc.1.push(router_route.clone());
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
        route: Vec<QuoterHop>,
    ) -> Option<U256>
    where
        DB: DBProvider + BlockHashReader + StateCommitmentProvider,
    {
        let profit_call = getProfitCall { amount, route };
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
                tracing::debug!(
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

    fn golden_section_search<DB: DBProvider + BlockHashReader + StateCommitmentProvider>(
        &self,
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
        min_liquidity: U256,
        max_liquidity: U256,
        hops: &[QuoterHop],
    ) -> Option<(U256, U256)> {
        let mut left = min_liquidity;
        let mut right = max_liquidity;

        let diff = right - left;
        let mut mid1 =
            right - (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
        let mut mid2 =
            left + (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);

        let (mut mid1_output, mut mid2_output) = join(
            || self.get_profit(latest_state_provider, mid1, hops.to_owned()),
            || self.get_profit(latest_state_provider, mid2, hops.to_owned()),
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
                mid2 = left
                    + (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
                mid2_output = self.get_profit(latest_state_provider, mid2, hops.to_owned());
                mid2_output?;
            } else {
                right = mid2;
                mid2 = mid1;
                mid2_output = mid1_output;

                let diff = right - left;
                mid1 = right
                    - (diff * U256::from(INV_GOLDEN_RATIO_NUM)) / U256::from(INV_GOLDEN_RATIO_DEN);
                mid1_output = self.get_profit(latest_state_provider, mid1, hops.to_owned());
                mid1_output?;
            }
        }

        if mid1_output.unwrap() >= mid2_output.unwrap() {
            Some((mid1, mid1_output.unwrap()))
        } else {
            Some((mid2, mid2_output.unwrap()))
        }
    }
}
