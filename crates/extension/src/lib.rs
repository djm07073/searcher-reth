pub mod exex;
pub mod strategy;

use eyre::{ Error, Result };
use revm::{ primitives::Bytes, state::Bytecode };

use clap::Args;
use strategy::path_finder::types::RoutePath;

pub struct SearcherExtension {
    pub(crate) contract: Bytecode,
    pub(crate) max_profit_ratio: u64,
    pub(crate) min_profit_ratio: u64,
    pub(crate) route_paths: Vec<RoutePath>,
}

#[derive(Debug, Clone, Args)]
pub struct SetupArgs {
    #[clap(long = "database-url", default_value = "")]
    pub database_url: String,

    #[clap(long = "bytecode", default_value = "")]
    pub bytecode: String,

    #[clap(long = "socket-path", default_value = "/tmp/ipc_socket")]
    pub socket_path: String,

    #[clap(long = "max-profit", default_value = "1000")] // 0.001%
    pub max_profit: Option<u64>,

    #[clap(long = "mint-profit", default_value = "500")] // 0.0005%
    pub min_profit: Option<u64>,
}

impl SearcherExtension {
    pub fn new(args: SetupArgs) -> Result<Self, Error> {
        let bytecode = args.bytecode.clone();
        let bytecode = Bytecode::new_raw_checked(Bytes(bytecode.into())).unwrap();
        Ok(Self {
            contract: bytecode,
            max_profit_ratio: args.max_profit.unwrap_or(1000),
            min_profit_ratio: args.min_profit.unwrap_or(500),
            route_paths: Vec::new(),
        })
    }

    pub fn update_contract(&mut self, bytecode: String) {
        self.contract = Bytecode::new_raw_checked(Bytes(bytecode.into())).unwrap();
    }

    pub fn update_profit_rate(&mut self, min_profit: Option<u64>, max_profit: Option<u64>) {
        self.min_profit_ratio = min_profit.unwrap_or(self.min_profit_ratio);
        self.max_profit_ratio = max_profit.unwrap_or(self.max_profit_ratio);
    }

    pub fn update_route_paths(&mut self, route_paths: Vec<RoutePath>) {
        self.route_paths = route_paths;
    }
}
