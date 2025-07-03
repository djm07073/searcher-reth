use std::collections::HashMap;

use alloy_primitives::{address, Address, FixedBytes, U256};
use alloy_rpc_types::{AccessList, AccessListItem};
use alloy_sol_types::SolStruct;
use eyre::Error;
use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    primitives::HashSet,
    state::{Bytecode, EvmState},
};
use reth_transaction_pool::PoolTransaction;
use searcher_reth_config::{strategy::StrategyConfig, types::Candidate};

pub const STRATEGY_CONTRACT_ADDRESS: Address = address!("0000000000000000000000000000000000012345");

pub type DirtyStates = HashMap<Address, HashSet<U256>>;

pub trait Strategy<'a> {
    type Action: SolStruct + Clone;

    type DB: DBProvider + BlockHashReader + StateCommitmentProvider;

    /// Creates Strategy of PathFinder with the given provider and contract bytecode.
    fn new(config: &StrategyConfig) -> Self;

    fn set_last_state(&mut self, provider: LatestStateProviderRef<'a, Self::DB>);

    fn get_code(&self) -> Bytecode;

    /// Finds profitable candidates from the pending transactions and candidates.
    fn find_profitable_candidates<T: PoolTransaction>(
        &mut self,
        pending_txs: Vec<T>,
        candidates: Vec<Candidate>,
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error>;

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
