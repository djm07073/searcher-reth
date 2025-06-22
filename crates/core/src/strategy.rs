use alloy_primitives::{Address, FixedBytes, U256, address};
use alloy_rpc_types::{AccessList, AccessListItem};
use alloy_sol_types::SolStruct;
use eyre::Error;

use reth_provider::{BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider};
use reth_revm::{
    primitives::HashSet,
    state::{Bytecode, EvmState},
};
use reth_transaction_pool::PoolTransaction;
use searcher_reth_config::strategy::StrategyConfig;
use std::collections::HashMap;

pub const STRATEGY_CONTRACT_ADDRESS: Address = address!("0000000000000000000000000000000000012345");

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
        candidates: HashMap<Address, Vec<Vec<Self::Action>>>,
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error>;

    /// Gey Vault Address
    fn get_vault(&self) -> Address;

    /// Get vault balance for the given token address.
    fn get_vault_balance(&mut self, token: Address) -> U256;

    /// Check if the result state has any dirty state if yes return None or not return the clean
    /// states.
    fn collect_clean_states(
        result_states: &EvmState,
        dirty_states: &[EvmState],
    ) -> Option<Vec<AccessListItem>> {
        let dirty_keys: HashSet<(Address, U256)> = dirty_states // _ -> U256
            .iter()
            .flat_map(|state| {
                state.iter().filter(|(_, account)| account.is_touched()).flat_map(
                    |(addr, account)| account.storage.keys().map(move |key| (*addr, *key)),
                )
            })
            .collect();

        let mut clean_states = Vec::<AccessListItem>::new();
        for (address, account) in result_states.iter() {
            if !account.is_touched() {
                continue;
            }
            if account.storage.keys().any(|key| dirty_keys.contains(&(*address, *key))) {
                return None;
            }
            let clean_storage_keys: Vec<FixedBytes<32>> =
                account.storage.keys().map(|key| FixedBytes::<32>::from(*key)).collect();
            clean_states
                .push(AccessListItem { address: *address, storage_keys: clean_storage_keys });
        }
        if clean_states.is_empty() { None } else { Some(clean_states) }
    }
}
