pub mod rpc;
pub mod exex;
mod database;

use eyre::{ Error, Result };
use clap::Args;
use revm::{ primitives::Bytes, state::Bytecode };
use searcher_reth_path_finder::RoutePath;

// impl of WithLaunchContext
#[derive(Debug, Clone, Args)]
pub struct SearcherExtensionArgs {
    #[clap(long = "bytecode", default_value = "")]
    pub bytecode: Option<String>,

    #[clap(long = "max-profit", default_value = "1000")] // 0.001%
    pub max_profit: Option<u64>,

    #[clap(long = "mint-profit", default_value = "500")] // 0.0005%
    pub min_profit: Option<u64>,
}

pub struct SearcherExtension {
    pub(crate) contract: Bytecode,
    pub(crate) max_profit_ratio: u64,
    pub(crate) min_profit_ratio: u64,
    pub(crate) route_paths: Vec<RoutePath>,
}

impl SearcherExtension {
    pub fn new(args: SearcherExtensionArgs) -> Result<Self, Error> {
        let bytes = if let Some(bytecode) = args.bytecode {
            Bytes::from(bytecode.as_str())
        } else {
            Bytes::default()
        };
        let bytecode = Bytecode::new_raw_checked(bytes).unwrap();
        // TODO: fetch the tokens and dexs from database and make paths for routing.
        let route_paths = vec![];
        Ok(Self {
            contract: bytecode,
            max_profit_ratio: args.max_profit.unwrap_or(1000),
            min_profit_ratio: args.min_profit.unwrap_or(500),
            route_paths,
        })
    }

    pub fn update_code(&mut self, bytecode: Bytecode) {
        self.contract = bytecode;
    }

    pub fn update_profit_rate(&mut self, min_profit: u64, max_profit: u64) {
        self.min_profit_ratio = min_profit;
        self.max_profit_ratio = max_profit;
    }
}
// launch context
// pub fn install_searcher_extension(ctx: &mut WithLaunchContext<NodeBuilderWithComponents<T, CB, AO>>) -> Self {

// }
