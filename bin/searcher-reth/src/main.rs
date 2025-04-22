use reth_node_ethereum::EthereumNode;

#[derive(Debug, Clone, Args)]
pub struct SearcherExtensionArgs {
    #[clap(long = "search-chain-id")]
    pub search_chain_id: String,

    #[clap(long = "bytecode", default_value = "")]
    pub bytecode: Option<String>,

    #[clap(long = "max-profit", default_value = "1000")] // 0.001%
    pub max_profit: Option<u64>,

    #[clap(long = "mint-profit", default_value = "500")] // 0.0005%
    pub min_profit: Option<u64>,
}

fn main() -> eyre::Result<()> {
    reth::cli::Cli::<EthereumChainSpecParser, SearcherExtensionArgs>
        ::parse_args()
        .run(|builder, args| async move {
            let handle = builder
                .node(EthereumNode::default())
                .install_searcher_extension(args)
                .launch().await?;

            handle.wait_for_node_exit().await
        }).await?;
}
