#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCodeParameters {
    pub bytecode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfitRateParameters {
    pub min_profit: Optional<u64>,
    pub max_profit: Optional<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigParameters {
    pub new_tokens: Optional<Vec<Address>>,
    pub deprecated_tokens: Optional<Vec<Address>>,
    pub new_dexs: Optional<Vec<DexType, Address>>,
    pub deprecated_dexs: Optional<Vec<DexType, Address>>,
}

#[rpc(server, namespace = "searcher")]
pub trait SearcherRpcApi {
    /// Set searcher contract
    #[method(name = "update_code")]
    async fn update_code(&self, params: SetCodeParameters) -> RpcResult<()>;

    /// Set range of profit rates
    #[method(name = "update_profit_rate")]
    async fn update_profit_rate(&self, params: SetProfitRateParameters) -> RpcResult<()>;

    // Update config of dex and token in in-memory and storage
    #[method(name = "update_config")]
    async fn update_config(&self, params: UpdateConfigParameters) -> RpcResult<()>;
}
