use std::collections::HashMap;

use alloy_primitives::{ Address, U256 };
use alloy_sol_types::{ SolCall, SolValue, sol };
use eyre::{ Error, Ok };

use reth_provider::{ BlockHashReader, DBProvider, StateCommitmentProvider };
use reth_revm::SystemCallEvm;
use reth_tracing::tracing::info;
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
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> Result<Vec<RoutePath>, Error> {
        let mut optimal_paths = Vec::<RoutePath>::new();
        while let Some(candidate) = candidates.iter().next() {
            if
                self.transact_route_paths(
                    vault,
                    &mut optimal_paths,
                    candidate.clone(),
                    max_profit_ratio,
                    min_profit_ratio
                )
            {
                break;
            }
        }

        Ok(optimal_paths)
    }

    fn transact_route_paths(
        &mut self,
        vault: Address,
        optimal_paths: &mut Vec<RoutePath>,
        route_paths: HashMap<Address, Vec<RoutePath>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> bool {
        for (starting_token, paths) in route_paths.iter() {
            let balance = self.get_vault_balance(vault, *starting_token);
            if balance.is_zero() {
                info!(
                    target = "reth-extension",
                    info = "balance zero",
                    starting_token = starting_token.to_string()
                );
                continue;
            }
            for route_path in paths.iter() {
                let encoded_data = (getProfitCall {
                    initialAmt: balance,
                    route: route_path.clone().into(),
                }).abi_encode();
                let result = self.evm
                    .transact_system_call(encoded_data.into(), DEPLOYED_ADDRESS)
                    .unwrap();
                // amount
                let net_profit = match result.result {
                    ExecutionResult::Success { output: Output::Call(value), .. } =>
                        <U256>::abi_decode(&value).unwrap(),
                    _ => {
                        continue;
                    }
                };
                let net_profit = net_profit.checked_div(balance).unwrap();

                if net_profit.ge(&max_profit_ratio) {
                    optimal_paths.push(route_path.clone());
                    return true;
                } else if net_profit.ge(&min_profit_ratio) {
                    optimal_paths.push(route_path.clone());
                }
            }
        }

        // it's enought to execute swap route paths
        if optimal_paths.len() > 10 {
            return true;
        }
        false
    }
}
