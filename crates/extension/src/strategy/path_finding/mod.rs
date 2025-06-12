pub mod types;

use alloy_primitives::{ Address, U256 };
use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth_revm::{
    Context,
    MainBuilder,
    MainContext,
    context::{ BlockEnv, CfgEnv, Evm, TxEnv },
    database::StateProviderDatabase,
    db::CacheDB,
    handler::{ EthPrecompiles, instructions::EthInstructions },
    interpreter::interpreter::EthInterpreter,
    state::{ AccountInfo, Bytecode },
};
use alloy_sol_types::{ SolCall, SolValue, sol };
use eyre::{ Error, Ok, Result };
use rayon::prelude::*;
use reth_transaction_pool::PoolTransaction;
use types::STRATEGY_CONTRACT_ADDRESS;
use std::{ collections::HashMap, sync::{ Arc, Mutex, atomic::{ AtomicBool, Ordering } } };
use reth_revm::SystemCallEvm;
use revm::{ context::result::{ ExecutionResult, Output }, primitives::HashSet, state::EvmState };

use crate::strategy::path_finding::types::getProfitCall;

pub use types::RoutePath;

use super::strategy::Strategy;

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

impl<'a, DB> PathFinder<'a, DB> where DB: DBProvider + BlockHashReader + StateCommitmentProvider {
    /// Creates a new instance of the PathFinder
    pub fn new(provider: LatestStateProviderRef<'a, DB>, contract: Bytecode) -> Self {
        let mut db = CacheDB::new(StateProviderDatabase::new(provider));
        db.insert_account_info(STRATEGY_CONTRACT_ADDRESS, AccountInfo {
            code_hash: contract.hash_slow(),
            code: Some(contract),
            ..Default::default()
        });
        let evm = Context::mainnet().with_db(db).build_mainnet();
        Self { evm }
    }
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
            ExecutionResult::Success { output: Output::Call(value), .. } => {
                <U256>::abi_decode(&value).unwrap()
            }
            _ => U256::ZERO,
        }
    }

    fn filter_candidates<T: PoolTransaction>(
        &mut self,
        vault: Address,
        pending_txs: Vec<T>,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> Result<Vec<RoutePath>, Error> {
        let mut balances: HashMap<Address, U256> = HashMap::new();
        // Get balances for all tokens in the candidate paths
        for candidate_map in candidates.clone() {
            for token in candidate_map.keys() {
                balances.insert(*token, self.get_vault_balance(vault, *token));
            }
        }
        // 1. Get dirty states from pending transactions
        let pevm = Arc::new(Mutex::new(&mut self.evm));
        let mut dirty_states: Vec<EvmState> = Vec::new();
        for tx in pending_txs.iter() {
            if let Some(to) = tx.to() {
                let mut evm = pevm.lock().unwrap();
                let data = tx.input().clone();
                let result = evm.transact_system_call(data.into(), to).unwrap();

                if has_dirty_state(&result.state, &dirty_states) {
                    dirty_states.push(result.state);
                }
            }
        }

        // 2. Filter candidates based on vault balances and profit ratios
        let mut filtered_candidates = Vec::<RoutePath>::new();
        for candidate_map in candidates {
            let filtered_paths = Arc::new(Mutex::new(Vec::new()));
            let found_max_profit = Arc::new(AtomicBool::new(false));

            candidate_map.par_iter().for_each(|(initial_token, paths)| {
                let balance = balances[initial_token];
                if balance.is_zero() {
                    return;
                }

                paths.par_iter().for_each(|path| {
                    if found_max_profit.load(Ordering::Relaxed) {
                        return;
                    }

                    let encoded_data = (getProfitCall {
                        initialAmt: balance,
                        route: path.clone(),
                    }).abi_encode();

                    let result = {
                        let mut evm = pevm.lock().unwrap();
                        let result = evm
                            .transact_system_call(encoded_data.into(), STRATEGY_CONTRACT_ADDRESS)
                            .unwrap();

                        if has_dirty_state(&result.state, &dirty_states) {
                            return;
                        }
                        result
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

            filtered_candidates.extend(
                Arc::try_unwrap(filtered_paths).unwrap().into_inner().unwrap()
            );

            if Arc::try_unwrap(found_max_profit).unwrap().load(Ordering::Relaxed) {
                break;
            }
        }

        Ok(filtered_candidates)
    }
}

// Updated evm state of result has already been applied to the dirty state
// filter out the paths that do not yield profit
fn has_dirty_state(result_state: &EvmState, dirty_states: &Vec<EvmState>) -> bool {
    if dirty_states.is_empty() {
        return false;
    }

    let dirty_keys: HashSet<(Address, _)> = dirty_states
        .iter()
        .flat_map(|state| {
            state
                .iter()
                .filter(|(_, account)| account.is_touched())
                .flat_map(|(addr, account)| {
                    account.storage.keys().map(move |key| (*addr, *key))
                })
        })
        .collect();

    result_state
        .iter()
        .filter(|(_, account)| account.is_touched())
        .any(|(address, account)| {
            account.storage.keys().any(|key| { dirty_keys.contains(&(*address, *key)) })
        })
}
