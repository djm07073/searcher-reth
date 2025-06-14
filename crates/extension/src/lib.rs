pub mod exex;
pub mod relayer_pool;
pub mod strategy;
use std::collections::HashMap;

use alloy_primitives::{Address, U256};
use eyre::{Error, Result};
use revm::{primitives::Bytes, state::Bytecode};

use clap::Args;
use strategy::path_finding::types::RoutePath;

pub struct SearcherExtension {
    pub(crate) vault: Address,
    pub(crate) contract: Bytecode,
    pub(crate) max_profit_ratio: U256,
    pub(crate) min_profit_ratio: U256,
    pub(crate) candidates: Vec<HashMap<Address, Vec<RoutePath>>>,
}

const ONE_ETHER: u128 = 1_000_000_000_000_000_000;

#[derive(Debug, Clone, Args)]
pub struct SetupArgs {
    #[clap(long = "bytecode", default_value = "")]
    pub bytecode: String,

    #[clap(long = "max-profit", default_value = "0.001")] // 0.001%
    pub max_profit: String,

    #[clap(long = "mint-profit", default_value = "0.0005")] // 0.0005%
    pub min_profit: String,
}

impl SearcherExtension {
    pub fn new(vault: Address, args: SetupArgs) -> Result<Self, Error> {
        let bytecode = args.bytecode.clone();
        let bytecode = Bytecode::new_raw_checked(Bytes(bytecode.into())).unwrap();
        Ok(Self {
            vault,
            contract: bytecode,
            max_profit_ratio: U256::from(
                (((args.max_profit.parse::<f64>().unwrap() * 1_000_000.0) as u128) * ONE_ETHER)
                    / 1_000_000,
            ),
            min_profit_ratio: U256::from(
                (((args.min_profit.parse::<f64>().unwrap() * 1_000_000.0) as u128) * ONE_ETHER)
                    / 1_000_000,
            ),
            candidates: Vec::new(),
        })
    }

    pub fn update_initial_value(&mut self, bytecode: String) {
        self.contract = Bytecode::new_raw_checked(Bytes(bytecode.into())).unwrap();
    }

    pub fn update_contract(&mut self, bytecode: String) {
        self.contract = Bytecode::new_raw_checked(Bytes(bytecode.into())).unwrap();
    }

    pub fn update_profit_rate(&mut self, min_profit: Option<String>, max_profit: Option<String>) {
        if let Some(min_profit) = min_profit {
            let min_profit = U256::from(
                (((min_profit.parse::<f64>().unwrap() * 1_000_000.0) as u128) * ONE_ETHER)
                    / 1_000_000,
            );
            self.min_profit_ratio = min_profit;
        }

        if let Some(max_profit) = max_profit {
            let max_profit = U256::from(
                (((max_profit.parse::<f64>().unwrap() * 1_000_000.0) as u128) * ONE_ETHER)
                    / 1_000_000,
            );

            self.max_profit_ratio = max_profit;
        }
    }

    pub fn update_candidates(&mut self, candidates: Vec<HashMap<Address, Vec<RoutePath>>>) {
        self.candidates = candidates;
    }
}
