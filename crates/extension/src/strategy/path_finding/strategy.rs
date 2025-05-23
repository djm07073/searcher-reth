use alloy_primitives::{Address, U256};
use alloy_sol_types::{SolCall, SolValue, sol};
use eyre::{Error, Ok};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use reth_provider::{BlockHashReader, DBProvider, StateCommitmentProvider};
use reth_revm::SystemCallEvm;
use revm::context::result::{ExecutionResult, Output};

use crate::strategy::path_finding::types::getProfitCall;

use super::{PathFinder, STRATEGY_CONTRACT_ADDRESS, types::RoutePath};

pub trait Strategy {
    fn filter_candidates(
        &mut self,
        vault: Address,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256,
    ) -> Result<Vec<RoutePath>, Error>;

    fn get_vault_balance(&mut self, vault: Address, token: Address) -> U256;
}

impl<'a, DB> Strategy for PathFinder<'a, DB>
where
    DB: DBProvider + BlockHashReader + StateCommitmentProvider,
{
    fn get_vault_balance(&mut self, vault: Address, token: Address) -> U256 {
        sol! {
            function balanceOf(address account) external view returns (uint256);
        }
        let encoded = (balanceOfCall { account: vault }).abi_encode();
        let result = self.evm.transact_system_call(encoded.into(), token).unwrap();
        match result.result {
            ExecutionResult::Success { output: Output::Call(value), .. } => {
                <U256>::abi_decode(&value).unwrap()
            }
            _ => U256::ZERO,
        }
    }

    fn filter_candidates(
        &mut self,
        vault: Address,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256,
    ) -> Result<Vec<RoutePath>, Error> {
        let mut filtered_candidates = Vec::<RoutePath>::new();

        for candidate in candidates {
            // Get balances for all tokens in the candidate paths
            let balances: HashMap<Address, U256> = {
                let mut balances = HashMap::new();
                for token in candidate.keys() {
                    balances.insert(*token, self.get_vault_balance(vault, *token));
                }
                balances
            };

            let filtered_paths = Arc::new(Mutex::new(Vec::new()));
            let found_max_profit = Arc::new(AtomicBool::new(false));
            let evm_state = Arc::new(Mutex::new(&mut self.evm));

            candidate.par_iter().for_each(|(initial_token, paths)| {
                let balance = balances[initial_token];
                if balance.is_zero() {
                    return;
                }

                paths.par_iter().for_each(|path| {
                    if found_max_profit.load(Ordering::Relaxed) {
                        return;
                    }

                    let encoded_data =
                        (getProfitCall { initialAmt: balance, route: path.clone() }).abi_encode();

                    let result = {
                        let mut evm = evm_state.lock().unwrap();
                        evm.transact_system_call(encoded_data.into(), STRATEGY_CONTRACT_ADDRESS)
                            .unwrap()
                    };

                    let net_profit = match result.result {
                        ExecutionResult::Success { output: Output::Call(value), .. } => {
                            <U256>::abi_decode(&value).unwrap()
                        }

                        _ => {
                            return;
                        }
                    };

                    let net_profit_ratio = net_profit.checked_div(balance).unwrap();
                    let mut paths = filtered_paths.lock().unwrap();

                    if net_profit_ratio.ge(&max_profit_ratio) {
                        paths.push(path.clone());
                        found_max_profit.store(true, Ordering::Relaxed);
                    } else if net_profit_ratio.ge(&min_profit_ratio) {
                        paths.push(path.clone());
                    }
                });
            });

            filtered_candidates
                .extend(Arc::try_unwrap(filtered_paths).unwrap().into_inner().unwrap());

            if Arc::try_unwrap(found_max_profit).unwrap().load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(filtered_candidates)
    }
}
