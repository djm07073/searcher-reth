pub mod types;

use alloy_primitives::{ Address, U256 };
use alloy_sol_types::{ SolCall, SolValue, sol };
use eyre::{ Error, Ok, Result };
use rayon::prelude::*;
use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth_revm::{
    Context,
    MainBuilder,
    MainContext,
    SystemCallEvm,
    context::{ BlockEnv, CfgEnv, Evm, TxEnv },
    database::StateProviderDatabase,
    db::CacheDB,
    handler::{ EthPrecompiles, instructions::EthInstructions },
    interpreter::interpreter::EthInterpreter,
    state::{ AccountInfo, Bytecode },
};
use reth_transaction_pool::PoolTransaction;
use revm::{ context::result::{ ExecutionResult, Output }, state::EvmState };
use std::{ collections::HashMap, sync::{ Arc, Mutex, atomic::{ AtomicBool, Ordering } } };
use types::{ executeCall, Hop };
use reth_tracing::tracing;

use crate::strategy::path_finding::types::getProfitCall;

use super::core::{ Strategy, STRATEGY_CONTRACT_ADDRESS };

type PathFinderCtx<'a, DB> = Context<
    BlockEnv,
    TxEnv,
    CfgEnv,
    CacheDB<StateProviderDatabase<LatestStateProviderRef<'a, DB>>>
>;

pub struct PathFinder<'a, DB> where DB: DBProvider + BlockHashReader + StateCommitmentProvider {
    evm: Evm<
        PathFinderCtx<'a, DB>,
        (),
        EthInstructions<EthInterpreter, PathFinderCtx<'a, DB>>,
        EthPrecompiles
    >,
}

impl<'a, StrategyDB> Strategy<'a>
    for PathFinder<'a, StrategyDB>
    where StrategyDB: DBProvider + BlockHashReader + StateCommitmentProvider
{
    type Action = Hop;

    type DB = StrategyDB;

    /// Creates a new instance of the PathFinder
    fn create(provider: LatestStateProviderRef<'a, StrategyDB>, contract: Bytecode) -> Self {
        let mut db = CacheDB::new(StateProviderDatabase::new(provider));
        db.insert_account_info(STRATEGY_CONTRACT_ADDRESS, AccountInfo {
            code_hash: contract.hash_slow(),
            code: Some(contract),
            ..Default::default()
        });
        let evm = Context::mainnet().with_db(db).build_mainnet();
        Self { evm }
    }

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

    fn find_profitable_candidates<T: PoolTransaction>(
        &mut self,
        vault: Address,
        pending_txs: Vec<T>,
        candidates: Vec<HashMap<Address, Vec<Vec<Self::Action>>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> Result<Vec<u8>, Error> {
        let mut balances: HashMap<Address, U256> = HashMap::new();
        // Get balances for all tokens in the candidate paths
        for candidate_map in candidates.clone() {
            for token in candidate_map.keys() {
                balances.insert(*token, self.get_vault_balance(vault, *token));
            }
        }
        // 1. Get dirty states from pending transactions
        let pevm = Arc::new(Mutex::new(&mut self.evm));
        let dirty_states: Vec<EvmState> = pending_txs
            .par_iter()
            .filter_map(|tx| {
                if let Some(to) = tx.to() {
                    let mut evm = pevm.lock().unwrap();
                    let data = tx.input().clone();
                    let result = evm.transact_system_call(data, to).unwrap();
                    Some(result.state)
                } else {
                    None
                }
            })
            .collect();

        // 2. Filter candidates based on vault balances and profit ratios
        let found_max_profit = Arc::new(AtomicBool::new(false));
        let profitable_candidates: Vec<Vec<Self::Action>> = candidates
            .par_iter()
            .take_any_while(|_| !found_max_profit.load(Ordering::Relaxed))
            .flat_map(|candidate| {
                candidate.par_iter().filter_map(|(initial_token, paths)| {
                    let balance = balances[initial_token];
                    if balance.is_zero() {
                        return None;
                    }

                    paths.par_iter().find_map_first(|path| {
                        if found_max_profit.load(Ordering::Relaxed) {
                            return None;
                        }

                        let encoded_data = (getProfitCall {
                            initialAmt: balance,
                            route: path.clone(),
                        }).abi_encode();

                        let result = {
                            let mut evm = pevm.lock().unwrap();
                            let result = evm
                                .transact_system_call(
                                    encoded_data.into(),
                                    STRATEGY_CONTRACT_ADDRESS
                                )
                                .unwrap();

                            if Self::has_dirty_state(&result.state, &dirty_states) {
                                return None;
                            }
                            result
                        };

                        let net_profit = match result.result {
                            ExecutionResult::Success { output: Output::Call(value), .. } => {
                                <U256>::abi_decode(&value).unwrap()
                            }
                            _ => {
                                return None;
                            }
                        };

                        let net_profit_ratio = net_profit.checked_div(balance).unwrap();

                        if net_profit_ratio.ge(&max_profit_ratio) {
                            found_max_profit.store(true, Ordering::Relaxed);
                            Some(path.clone())
                        } else if net_profit_ratio.ge(&min_profit_ratio) {
                            Some(path.clone())
                        } else {
                            None
                        }
                    })
                })
            })
            .collect();

        // Log the routes being sent
        let routes = profitable_candidates
            .clone()
            .iter()
            .map(|route| format!("{:?}", route))
            .collect::<Vec<String>>();
        let route_len = routes.len();
        tracing::info!(
            target: "reth-exex",
            action = "send_candidates_to_relayer_pool",
            route_len = route_len,
            routes = routes.join(", "),
            "Sending encoded calldata to socket"
        );

        // Encode the profitable candidates into calldata
        let calldata = (executeCall {
            routes: profitable_candidates.clone(),
        }).abi_encode();

        Ok(calldata)
    }

    fn has_dirty_state(result_state: &EvmState, dirty_states: &[EvmState]) -> bool {
        if dirty_states.is_empty() {
            return false;
        }

        let dirty_keys: revm::primitives::HashSet<(Address, _)> = dirty_states
            .iter()
            .flat_map(|state| {
                state
                    .iter()
                    .filter(|(_, account)| account.is_touched())
                    .flat_map(|(addr, account)|
                        account.storage.keys().map(move |key| (*addr, *key))
                    )
            })
            .collect();

        result_state
            .iter()
            .filter(|(_, account)| account.is_touched())
            .any(|(address, account)| {
                account.storage.keys().any(|key| dirty_keys.contains(&(*address, *key)))
            })
    }
}
