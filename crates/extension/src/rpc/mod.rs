// update rpc endpoint
// dexs / tokens / simulate contract bytecode

// case 1: dexs / tokens => update route paths
// total number of paths: d*(d-1)*n + d*(d-1)*(d-2)*mC2 + d*(d-1)*(d-2)*(d-3)*mC3
// case 2: simulate contract => update bytecode
#[async_trait]
impl<P> SearcherRpcApiServer
    for SearcherExtenstion<P>
    where P: BlockNumReader + BlockReaderIdExt + Clone + Unpin + 'static
{
    async fn set_code(&self, params: SetCodeParameters) -> RpcResult<()> {
        todo!()
    }

    async fn update_config(&self, params: UpdateConfigParameters) -> RpcResult<()> {
        todo!()
    }
}
