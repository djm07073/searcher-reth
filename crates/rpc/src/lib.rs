use std::{ collections::HashMap, sync::Arc };

use jsonrpsee::{ core::{ RpcResult, async_trait }, proc_macros::rpc, tracing::info };
use reth_revm::primitives::Address;
use searcher_reth_extension::{
    strategy::path_finding::{ types::Hop, RoutePath },
    SearcherExtension,
};
use searcher_reth_repository::SearcherRepository;
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
    pub min_profit: Option<String>,
    pub max_profit: Option<String>,
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
    async fn update_route_paths(&self) -> RpcResult<()>;
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
        let mut candidates: Vec<HashMap<Address, Vec<RoutePath>>> = Vec::new();
        let route_paths = repo.get_route_paths(chain_id).unwrap();
        // convert hop::Model to Hop
        for (dex, paths) in route_paths {
            let mut dex_map: HashMap<Address, Vec<RoutePath>> = HashMap::new();
            for path in paths {
                let route_path: Vec<Hop> = path.into_iter().map(Hop::from).collect();
                dex_map.entry(dex).or_default().push(route_path);
            }
            candidates.push(dex_map);
        }
        extension.write().await.update_candidates(candidates);
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
        let repo = self.repo.clone();
        let chain_id = self.chain_id;
        let bytecode = params.bytecode.clone();
        self.extension.write().await.update_contract(params.bytecode);

        let _ = tokio::task::spawn_blocking(move || {
            repo.update_contract(chain_id, bytecode.clone()).unwrap();
            info!(
                event = "contract_code_updated",
                chain_id = chain_id,
                bytecode = bytecode,
                "Contract bytecode has been updated"
            );
        });
        Ok(())
    }

    async fn update_profit_rate(&self, params: UpdateProfitRateParameters) -> RpcResult<()> {
        info!(
            event = "profit_rate_updated",
            min_profit = ?params.min_profit,
            max_profit = ?params.max_profit,
            "Profit rate parameters have been updated"
        );
        self.extension.write().await.update_profit_rate(params.min_profit, params.max_profit);
        Ok(())
    }

    async fn update_route_paths(&self) -> RpcResult<()> {
        let repo = self.repo.clone();
        let extension = self.extension.clone();
        let chain_id = self.chain_id;
        let _ = tokio::task::spawn(async move {
            let mut candidates: Vec<HashMap<Address, Vec<RoutePath>>> = Vec::new();
            let route_paths = repo.get_route_paths(chain_id).unwrap();
            // convert hop::Model to Hop
            for (dex, paths) in route_paths {
                let mut dex_map: HashMap<Address, Vec<RoutePath>> = HashMap::new();
                for path in paths {
                    let route_path: Vec<Hop> = path.into_iter().map(Hop::from).collect();
                    dex_map.entry(dex).or_default().push(route_path);
                }
                candidates.push(dex_map);
            }

            extension.write().await.update_candidates(candidates);
            info!(target = "searcher_rpc", "Updated route paths for chain_id");
        });

        Ok(())
    }
}
