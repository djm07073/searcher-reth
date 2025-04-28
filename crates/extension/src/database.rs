// ! update tokens and dexs

use std::future::Future;

use eyre::Result;
use revm::primitives::Address;
use searcher_reth_path_finder::DexType;

// tokens: chain_id, score, address
// dexs: chain_id, dex type, address

pub(crate) type TokenScore = u64;
pub trait SearcherDatabase {
    // get all of tokens in specific chain id sorted by score
    async fn get_all_tokens(&self, chain_id: u64) -> Result<impl Future<Output = Result<()>>>;

    async fn get_all_dexs(&self, chain_id: u64) -> Result<impl Future<Output = Result<()>>>;

    async fn insert_tokens(
        &self,
        chain_id: u64,
        tokens: Option<Vec<(Address, TokenScore)>>
    ) -> Result<impl Future<Output = Result<()>>>;

    async fn insert_dexs(
        &self,
        chain_id: u64,
        dexs: Option<Vec<(Address, DexType)>>
    ) -> Result<impl Future<Output = Result<()>>>;

    async fn delete_tokens(
        &self,
        chain_id: u64,
        tokens: Option<Vec<Address>>
    ) -> Result<impl Future<Output = Result<()>>>;

    async fn delete_dexs(
        &self,
        chain_id: u64,
        dexs: Option<Vec<Address>>
    ) -> Result<impl Future<Output = Result<()>>>;
}
