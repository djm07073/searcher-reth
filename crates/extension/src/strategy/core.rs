use alloy_primitives::{Address, U256};
use eyre::Error;
use reth_transaction_pool::PoolTransaction;
use revm::{primitives::HashSet, state::EvmState};
use std::collections::HashMap;

use super::path_finding::RoutePath;

pub trait Strategy {
    fn find_profitable_candidates<T: PoolTransaction>(
        &mut self,
        vault: Address,
        pending_txs: Vec<T>,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        // TODO: find function parameters
        max_profit_ratio: U256,
        min_profit_ratio: U256,
    ) -> Result<Vec<RoutePath>, Error>;

    fn get_vault_balance(&mut self, vault: Address, token: Address) -> U256;

    // Updated evm state of result has already been applied to the dirty state
    // filter out the paths that do not yield profit
    fn has_dirty_state(result_state: &EvmState, dirty_states: &[EvmState]) -> bool {
        if dirty_states.is_empty() {
            return false;
        }

        let dirty_keys: HashSet<(Address, _)> = dirty_states
            .iter()
            .flat_map(|state| {
                state.iter().filter(|(_, account)| account.is_touched()).flat_map(
                    |(addr, account)| account.storage.keys().map(move |key| (*addr, *key)),
                )
            })
            .collect();

        result_state.iter().filter(|(_, account)| account.is_touched()).any(|(address, account)| {
            account.storage.keys().any(|key| dirty_keys.contains(&(*address, *key)))
        })
    }
}
