use std::collections::HashMap;

use alloy_primitives::{Address, FixedBytes, U256, address};
use alloy_rpc_types::{AccessList, AccessListItem};
use alloy_sol_types::SolStruct;
use eyre::Error;
use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    Context, MainBuilder, MainContext, SystemCallEvm,
    context::result::ResultAndState,
    database::StateProviderDatabase,
    db::CacheDB,
    primitives::HashSet,
    state::{AccountInfo, Bytecode, EvmState},
};
use reth_transaction_pool::PoolTransaction;
use searcher_reth_config::{strategy::StrategyConfig, types::Candidate};

pub const STRATEGY_CONTRACT_ADDRESS: Address = address!("0000000000000000000000000000000000012345");

type StrategyContext<'a, DB> = Context<
    reth_revm::context::BlockEnv,
    reth_revm::context::TxEnv,
    reth_revm::context::CfgEnv,
    CacheDB<StateProviderDatabase<&'a LatestStateProviderRef<'a, DB>>>,
>;

pub type StrategyEvm<'a, DB> = reth_revm::context::Evm<
    StrategyContext<'a, DB>,
    (),
    reth_revm::handler::instructions::EthInstructions<
        reth_revm::interpreter::interpreter::EthInterpreter,
        StrategyContext<'a, DB>,
    >,
    reth_revm::handler::EthPrecompiles,
>;

pub type DirtyStates = HashMap<Address, HashSet<U256>>;

pub trait Strategy {
    type Action: SolStruct + Clone;

    /// Creates Strategy of PathFinder with the given provider and contract bytecode.
    fn new(config: &StrategyConfig) -> Self;

    fn get_code(&self) -> Bytecode;

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

    fn call_get_profit<'a, DB>(
        &self,
        provider: &'a LatestStateProviderRef<'a, DB>,
        encoded: Vec<u8>,
    ) -> Result<ResultAndState, Error>
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
        let result = evm.transact_system_call(encoded.into(), STRATEGY_CONTRACT_ADDRESS)?;
        Ok(result)
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
}
