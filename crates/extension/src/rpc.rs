use std::sync::Arc;

use jsonrpsee::{ core::{ async_trait, RpcResult }, proc_macros::rpc };
use reth_provider::{ BlockNumReader, BlockReaderIdExt };
use reth_revm::primitives::Address;
use searcher_reth_path_finder::DexType;
use serde::{ Deserialize, Serialize };

use crate::SearcherExtension;

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
pub struct UpdateConfigParameters {
    pub new_tokens: Option<Vec<(Address, u64)>>,
    pub deprecated_tokens: Option<Vec<Address>>,
    pub new_dexs: Option<Vec<(DexType, Address)>>,
    pub deprecated_dexs: Option<Vec<(DexType, Address)>>,
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
    #[method(name = "update_config")]
    async fn update_config(&self, params: UpdateConfigParameters) -> RpcResult<()>;
}

pub struct SearcherRpc<P> {
    provider: P,
    extension: Arc<SearcherExtension>,
}

impl<P> SearcherRpc<P> {
    pub fn new(provider: P, extension: Arc<SearcherExtension>) -> Self {
        Self { provider, extension }
    }
}

// update rpc endpoint
// dexs / tokens / simulate contract bytecode

// case 1: dexs / tokens => update route paths
// total number of paths: d*(d-1)*n + d*(d-1)*(d-2)*mC2 + d*(d-1)*(d-2)*(d-3)*mC3
// case 2: simulate contract => update bytecode
#[async_trait]
impl<P> SearcherRpcApiServer
    for SearcherRpc<P>
    where P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static
{
    async fn update_code(&self, params: UpdateCodeParameters) -> RpcResult<()> {
        todo!()
    }

    async fn update_profit_rate(&self, params: UpdateProfitRateParameters) -> RpcResult<()> {
        todo!()
    }

    async fn update_config(&self, params: UpdateConfigParameters) -> RpcResult<()> {
        todo!()
    }
}
