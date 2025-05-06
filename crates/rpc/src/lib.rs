use std::sync::Arc;

use jsonrpsee::{ core::{ async_trait, RpcResult }, proc_macros::rpc };
use reth_revm::primitives::Address;
use searcher_reth_extension::{
    strategy::path_finder::candidates::get_candidates,
    SearcherExtension,
};
use searcher_reth_repository::SearcherRepository;
use searcher_reth_types::DexType;
use serde::{ Deserialize, Serialize };
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCodeParameters {
    pub bytecode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfitRateParameters {
    pub min_profit: Option<u64>,
    pub max_profit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoutePathParameters {
    pub new_tokens: Option<Vec<(Address, u64)>>,
    pub deprecated_tokens: Option<Vec<Address>>,
    pub new_dexs: Option<Vec<(DexType, Address)>>,
    pub deprecated_dexs: Option<Vec<Address>>,
}

#[rpc(server, namespace = "searcher")]
pub trait SearcherRpcApi {
    /// Set searcher contract
    #[method(name = "update_code")]
    async fn update_code(&self, params: UpdateCodeParameters) -> RpcResult<()>;

    /// Set range of profit rates
    #[method(name = "update_profit_rate")]
    async fn update_profit_rate(&self, params: UpdateProfitRateParameters) -> RpcResult<()>;

    // Update config of dex and token in in-memory and storage
    #[method(name = "update_route_paths")]
    async fn update_route_paths(&self, params: UpdateRoutePathParameters) -> RpcResult<()>;
}

pub struct SearcherRpc {
    chain_id: u64,
    extension: Arc<RwLock<SearcherExtension>>,
    repo: Arc<SearcherRepository>,
}

impl SearcherRpc {
    pub async fn new(
        chain_id: u64,
        extension: Arc<RwLock<SearcherExtension>>,
        repo: Arc<SearcherRepository>
    ) -> Self {
        let dexs = repo.get_all_dexs(chain_id).await.unwrap();
        let tokens = repo.get_all_tokens(chain_id).await.unwrap();
        let route_paths = get_candidates(dexs, tokens);
        extension.write().await.update_route_paths(route_paths);
        Self { chain_id, extension, repo }
    }
}

// update rpc endpoint
// dexs / tokens / simulate contract bytecode

// case 1: dexs / tokens => update route paths
// total number of paths: d*(d-1)*n + d*(d-1)*(d-2)*mC2 + d*(d-1)*(d-2)*(d-3)*mC3
// case 2: simulate contract => update bytecode
#[async_trait]
impl SearcherRpcApiServer for SearcherRpc {
    async fn update_code(&self, params: UpdateCodeParameters) -> RpcResult<()> {
        // update repository
        self.repo.update_contract(self.chain_id, params.bytecode.clone()).await.unwrap();
        // update extension
        self.extension.write().await.update_contract(params.bytecode);
        Ok(())
    }

    async fn update_profit_rate(&self, params: UpdateProfitRateParameters) -> RpcResult<()> {
        // only update extension
        self.extension.write().await.update_profit_rate(params.min_profit, params.max_profit);
        Ok(())
    }

    async fn update_route_paths(&self, params: UpdateRoutePathParameters) -> RpcResult<()> {
        // Implement the logic to update the configuration of dex and tokens
        let route_paths = vec![];
        // update repository
        self.repo
            .update_route_paths(
                self.chain_id,
                &params.new_tokens,
                &params.deprecated_tokens,
                &params.new_dexs,
                &params.deprecated_dexs
            ).await
            .unwrap();
        self.extension.write().await.update_route_paths(route_paths);
        Ok(())
    }
}
