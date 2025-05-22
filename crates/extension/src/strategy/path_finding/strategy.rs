use std::{ collections::HashMap, sync::{ Arc, Mutex } };
use rayon::prelude::*;
use alloy_primitives::{ Address, U256 };
use alloy_sol_types::{ SolCall, SolValue, sol };
use eyre::{ Error, Ok };

use reth_provider::{ BlockHashReader, DBProvider, StateCommitmentProvider };
use reth_revm::SystemCallEvm;
use revm::context::result::{ ExecutionResult, Output };

use crate::strategy::path_finding::types::getProfitCall;

use super::{ types::RoutePath, PathFinder, DEPLOYED_ADDRESS };

pub trait Strategy {
    fn filter_candidates(
        &mut self,
        vault: Address,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> Result<Vec<RoutePath>, Error>;

    fn get_vault_balance(&mut self, vault: Address, token: Address) -> U256;

    fn transact_route_paths(
        &mut self,
        vault: Address,
        optimal_paths: &mut Vec<RoutePath>,
        route_paths: HashMap<Address, Vec<RoutePath>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> bool;
}

impl<'a, DB> Strategy
    for PathFinder<'a, DB>
    where DB: DBProvider + BlockHashReader + StateCommitmentProvider
{
    fn get_vault_balance(&mut self, vault: Address, token: Address) -> U256 {
        sol! {
            function balanceOf(address account) external view returns (uint256);
        }
        let encoded = (balanceOfCall { account: vault }).abi_encode();
        let result = self.evm.transact_system_call(encoded.into(), token).unwrap();
        match result.result {
            ExecutionResult::Success { output: Output::Call(value), .. } =>
                <U256>::abi_decode(&value).unwrap(),
            _ => U256::ZERO,
        }
    }

    fn filter_candidates(
        &mut self,
        vault: Address,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>, // vec![hop2_paths, hop3_paths]
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> Result<Vec<RoutePath>, Error> {
        let mut filtered_candidates = Vec::<RoutePath>::new();
        while let Some(candidate) = candidates.first() {
            if
                self.transact_route_paths(
                    vault,
                    &mut filtered_candidates,
                    candidate.clone(),
                    max_profit_ratio,
                    min_profit_ratio
                )
            {
                break;
            }
        }

        Ok(filtered_candidates)
    }

    fn transact_route_paths(
        &mut self,
        vault: Address,
        filtered_candidates: &mut Vec<RoutePath>,
        initial_token_route_map: HashMap<Address, Vec<RoutePath>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> bool {
        let balances: HashMap<Address, U256> = {
            let mut balances = HashMap::new();
            for token in initial_token_route_map.keys() {
                balances.insert(*token, self.get_vault_balance(vault, *token));
            }
            balances
        };

        let filtered_paths = Arc::new(Mutex::new(Vec::new()));
        let found_max_profit = Arc::new(Mutex::new(false));
        let evm_state = Arc::new(Mutex::new(&mut self.evm));

        initial_token_route_map.par_iter().for_each(|(initial_token, paths)| {
            let balance = balances[initial_token];
            if balance.is_zero() {
                return;
            }

            paths.par_iter().for_each(|path| {
                if *found_max_profit.lock().unwrap() {
                    return;
                }

                let encoded_data = (getProfitCall {
                    initialAmt: balance,
                    route: path.clone(),
                }).abi_encode();

                let result = {
                    let mut evm = evm_state.lock().unwrap();
                    evm.transact_system_call(encoded_data.into(), DEPLOYED_ADDRESS).unwrap()
                };

                let net_profit = match result.result {
                    ExecutionResult::Success { output: Output::Call(value), .. } =>
                        <U256>::abi_decode(&value).unwrap(),
                    _ => {
                        return;
                    }
                };

                let net_profit_ratio = net_profit.checked_div(balance).unwrap();
                let mut paths = filtered_paths.lock().unwrap();

                if net_profit_ratio.ge(&max_profit_ratio) {
                    paths.push(path.clone());
                    *found_max_profit.lock().unwrap() = true;
                } else if net_profit_ratio.ge(&min_profit_ratio) {
                    paths.push(path.clone());
                }
            });
        });

        filtered_candidates.extend(Arc::try_unwrap(filtered_paths).unwrap().into_inner().unwrap());
        *found_max_profit.lock().unwrap()
    }
}
