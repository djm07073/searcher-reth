use std::sync::Arc;

use reth_node_ethereum::EthereumNode;
use searcher_reth_extension::{ exex::SearcherExEx, rpc::SearcherRpc, SearcherExtension };

fn main() -> eyre::Result<()> {
    // database
    reth::cli::Cli::<EthereumChainSpecParser, SearcherExtensionArgs>
        ::parse_args()
        .run(|builder, args| async move {
            let extension = Arc::new(SearcherExtension::new(args).unwrap());
            let handle = builder
                .node(EthereumNode::default())
                .extend_rpc_modules(move |ctx| {
                    let provider = ctx.provider().clone();
                    let searcher_rpc = std::thread
                        ::spawn(move || {
                            let rt = tokio::runtime::Runtime
                                ::new()
                                .expect("failed to spawn blocking runtime");
                            rt.block_on(SearcherRpc::new(provider, extension.clone()))
                        })
                        .join()
                        .map_err(|_| eyre!("failed to join SearcherRpc thread"))??;
                    ctx.modules
                        .merge_configured(searcher_rpc.into_rpc(()))
                        .map_err(|e| eyre!("failed to extend w/ SearcherRpc: {e}"))?;
                    Ok(())
                })
                .install_exex("SearcherExEx", move |ctx| {
                    let extension_ref = extension.as_ref();
                    let res = extension_ref.init(ctx);
                    res
                })
                .launch().await?;

            handle.wait_for_node_exit().await
        });

    Ok(())
}
