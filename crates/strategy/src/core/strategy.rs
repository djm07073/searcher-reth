use std::collections::HashMap;

use alloy_primitives::{Address, FixedBytes, U256, address, map::HashSet};
use alloy_rpc_types::{AccessList, AccessListItem};
use alloy_sol_types::SolStruct;
use eyre::Error;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    Context, ExecuteEvm, MainBuilder, MainContext,
    context::{
        TxEnv,
        result::{ExecResultAndState, ExecutionResult},
    },
    database::StateProviderDatabase,
    db::CacheDB,
    state::{AccountInfo, Bytecode, EvmState},
};
use reth_transaction_pool::PoolTransaction;
use searcher_reth_manager::{common::StrategyConfig, gas::GasConfig, types::Candidate};

pub const STRATEGY_CONTRACT_ADDRESS: Address = address!("0000000000000000000000000000000000012345");
pub const GAS_LIMIT: u64 = 1_000_000_000_000_000_000;
pub type DirtyStates = HashMap<Address, HashSet<U256>>;

pub trait Strategy {
    type Action: SolStruct + Clone;

    /// Creates Strategy of PathFinder with the given provider and contract bytecode.
    fn new(config: StrategyConfig) -> Self;

    fn gas_config(&self) -> GasConfig;

    fn get_or_load_candidates(&mut self, chain_id: u64) -> Vec<Candidate>;

    fn get_code(&self) -> Bytecode;

    fn get_vault(&self) -> Address;

    /// Finds profitable candidates from the pending transactions and candidates.
    fn find_profitable_candidates<T, DB>(
        &mut self,
        latest_state_provider: LatestStateProviderRef<'_, DB>,
        pending_txs: Vec<T>,
        candidates: Vec<Candidate>,
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error>
    where
        T: PoolTransaction,
        DB: DBProvider + BlockHashReader + StateCommitmentProvider;

    fn collect_dirty_states_from_pending_txs<T, DB>(
        pending_txs: Vec<T>,
        latest_state_provider: &LatestStateProviderRef<'_, DB>,
    ) -> DirtyStates
    where
        T: PoolTransaction,
        DB: DBProvider + BlockHashReader + StateCommitmentProvider,
    {
        pending_txs
            .par_iter()
            .filter_map(|tx| {
                let to = tx.to()?;
                let data = tx.input().clone();
                let db = CacheDB::new(StateProviderDatabase::new(latest_state_provider));
                let mut evm = Context::mainnet().with_db(db).build_mainnet();
                let tx_env = TxEnv::builder()
                    .caller(Address::ZERO)
                    .kind(alloy_primitives::TxKind::Call(to))
                    .data(data)
                    .value(U256::from(0))
                    .gas_limit(GAS_LIMIT)
                    .build()
                    .unwrap();
                let result = evm.transact(tx_env).ok()?;
                let dirty_state: DirtyStates = result
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
            .unwrap_or_default()
    }

    /// Check if the result state has any dirty state if yes return None or not return the clean
    /// states.
    fn collect_clean_states(
        result_states: &EvmState,
        dirty_states: &DirtyStates,
    ) -> Option<Vec<AccessListItem>> {
        let mut clean_states = Vec::<AccessListItem>::new();
        for (address, account) in result_states.iter() {
            if let Some(dirty_storage) = dirty_states.get(address) {
                if !account.is_touched() {
                    continue;
                }
                if account.storage.keys().any(|key| dirty_storage.contains(key)) {
                    return None;
                }
                let clean_storage_keys: Vec<FixedBytes<32>> =
                    account.storage.keys().map(|key| FixedBytes::<32>::from(*key)).collect();
                clean_states
                    .push(AccessListItem { address: *address, storage_keys: clean_storage_keys });
            }
        }
        if clean_states.is_empty() { None } else { Some(clean_states) }
    }

    fn call_get_profit<'a, DB>(
        &self,
        provider: &'a LatestStateProviderRef<'a, DB>,
        encoded: Vec<u8>,
    ) -> Result<ExecutionResult, Error>
    where
        DB: DBProvider + BlockHashReader + StateCommitmentProvider,
    {
        let contract = self.get_code();
        let mut db = CacheDB::new(StateProviderDatabase::new(provider));
        db.insert_account_info(
            STRATEGY_CONTRACT_ADDRESS,
            AccountInfo {
                code_hash: contract.hash_slow(),
                code: Some(contract.clone()),
                ..Default::default()
            },
        );

        let mut evm = Context::mainnet().with_db(db).build_mainnet();
        let tx_env = TxEnv::builder()
            .caller(Address::ZERO)
            .kind(alloy_primitives::TxKind::Call(STRATEGY_CONTRACT_ADDRESS))
            .data(encoded.into())
            .value(U256::from(0))
            .gas_limit(GAS_LIMIT)
            .build()
            .unwrap();
        let result = evm.transact_one(tx_env)?;
        Ok(result)
    }

    fn call_execute<'a, DB>(
        &self,
        provider: &'a LatestStateProviderRef<'a, DB>,
        encoded: Vec<u8>,
    ) -> Result<ExecResultAndState<ExecutionResult>, Error>
    where
        DB: DBProvider + BlockHashReader + StateCommitmentProvider,
    {
        let vault = self.get_vault();
        let db = CacheDB::new(StateProviderDatabase::new(provider));
        let mut evm = Context::mainnet().with_db(db).build_mainnet();
        let tx_env = TxEnv::builder()
            .caller(Address::ZERO)
            .kind(alloy_primitives::TxKind::Call(vault))
            .data(encoded.into())
            .value(U256::from(0))
            .gas_limit(GAS_LIMIT)
            .build()
            .unwrap();
        let result = evm.transact(tx_env)?;
        Ok(result)
    }
}
