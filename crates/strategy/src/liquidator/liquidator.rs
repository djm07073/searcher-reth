use crate::liquidator::db_writer::RocksDB;
use crate::liquidator::datasets::{
    aave_execute_borrow::process_aave_execute_borrow,
};
use alloy_consensus::{Header};
use eyre::Result;
use reth::builder::NodeTypes;
use reth::primitives::NodePrimitives;
use reth_primitives::{TransactionSigned, Receipt, RecoveredBlock, Block};
use reth_node_api::{ConfigureEvm, FullNodeComponents, FullNodeTypes};
use reth_tracing::tracing::{info, warn};
use std::time::Instant;
use searcher_reth_manager::{
    common::{ CommonStrategyConfig, ONE_ETHER, StrategyConfig },
    gas::GasConfig
};

// Structure to hold all the components needed for processing
#[derive(Clone)]
pub struct ProcessingComponents<Node: FullNodeComponents> {
    pub provider: Node::Provider,
    config: StrategyConfig,
}

struct ProcessorInfo<Node: FullNodeComponents> {
    table_name: &'static str,
    processor_name: &'static str,
    processor: for<'a> fn(
        &'a (RecoveredBlock<Block>, Vec<Receipt>),
        ProcessingComponents<Node>,
        &'a RocksDB
    ) -> futures::future::BoxFuture<'a, Result<()>>,
}

impl<Node: FullNodeComponents> ProcessorInfo<Node> {
    fn new(
        table_name: &'static str,
        processor_name: &'static str,
        processor: for<'a> fn(
            &'a (RecoveredBlock<Block>, Vec<Receipt>),
            ProcessingComponents<Node>,
            &'a RocksDB
        ) -> futures::future::BoxFuture<'a, Result<()>>,
    ) -> Self {
        Self {
            table_name,
            processor_name,
            processor,
        }
    }
}

pub struct Liquidator<Node: FullNodeComponents> {
    processors: Vec<ProcessorInfo<Node>>,
    config: StrategyConfig,
}

impl Strategy for Liquidator<Node> {
    fn new(config: Config) -> Self {
        let mut liquidator = Self {
            processors: Vec::new(),
            config,
        };

        liquidator.add_processor("aave_execute_borrow", "AaveExecuteBorrow");
        info!("Initialized liquidator with processors: {:?}", liquidator.list_processors());
        liquidator
    }

    fn gas_config(&self) -> GasConfig {
        self.config.get_gas_config()
    }

    fn get_code(&self) -> Bytecode {
        Bytecode::default()
    }

    fn get_vault(&self) -> Address {
        self.config.get_vault()
    }

    fn prepare(&mut self, chain_id: u64) {
        tracing::info!(
            target: "liquidator",
            event = "prepare_enter",
            chain_id = chain_id,
            "Entered prepare()"
        );
        // TODO : define actions for prepare stage
        tracing::info!(
            target: "liquidator",
            event = "prepare_exit",
            chain_id = chain_id,
            "Exiting prepare()"
        );
    }

    fn find_profitable_candidates<
        T: PoolTransaction,
        DB: DBProvider + BlockHashReader + StateCommitmentProvider
    >(
        &mut self,
        block: NumHash,
        latest_state_provider: LatestStateProviderRef<'_, DB>,
        pending_txs: Vec<T>
    ) -> Result<Option<(Vec<u8>, AccessList)>, Error> {
        // TODO : define actions for find_profitable_candidates stage
        // 1. get liquidations candidates from db
        // 2. filter candidates which can be liquidated
        // 3. return the candidates
        // 4. store new candidates that indexer processed
    }
}


