use alloy_primitives::{ Address, U256 };
use eyre::Error;
use reth_transaction_pool::PoolTransaction;
use std::collections::HashMap;

use super::path_finding::RoutePath;

pub trait Strategy {
    fn filter_candidates<T: PoolTransaction>(
        &mut self,
        vault: Address,
        pending_txs: Vec<T>,
        candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
        max_profit_ratio: U256,
        min_profit_ratio: U256
    ) -> Result<Vec<RoutePath>, Error>;

    fn get_vault_balance(&mut self, vault: Address, token: Address) -> U256;
}
