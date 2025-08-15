use crate::liquidator::db_writer::RocksDB;
use crate::{ core::strategy::Strategy};
use alloy_eips::NumHash;
use alloy_rpc_types::{ AccessList };
use eyre::{Result, Error};
use reth_revm::{
    state::Bytecode,
};
use reth_primitives::{TransactionSigned, Receipt, RecoveredBlock, Block, Header};
use alloy_primitives::{ Address};
use reth_node_api::{ConfigureEvm, FullNodeComponents};
use reth_tracing::tracing;
use reth_transaction_pool::PoolTransaction;
use reth_provider::{ BlockHashReader, DBProvider, LatestStateProviderRef, StateCommitmentProvider };
use reth::primitives::{ NodePrimitives};
use searcher_reth_manager::{
    common::{ CommonStrategyConfig, StrategyConfig },
    gas::GasConfig,
    types::{StrategyCandidates}
};
use reth::builder::NodeTypes;
use std::time::Instant;
use crate::liquidator::db_writer::TableName;
use crate::liquidator::datasets::{
    dolomite_borrow_position::process_dolomite_borrow_positions,
    aave_execute_borrow::process_aave_execute_borrow,
};
use crate::liquidator::LiquidatorTodoAction;

// Structure to hold all the components needed for processing
#[derive(Clone)]
pub struct ProcessingComponents<Node: FullNodeComponents> {
    pub provider: Node::Provider,
    config: StrategyConfig,
}

struct ProcessorInfo<Node: FullNodeComponents> {
    table_name: &'static str,
    processor_name: String,
    processor: for<'a> fn(
        &'a (RecoveredBlock<Block>, Vec<Receipt>),
        ProcessingComponents<Node>,
        &'a RocksDB
    ) -> futures::future::BoxFuture<'a, Result<()>>,
}

impl<Node: FullNodeComponents> ProcessorInfo<Node> {
    fn new(
        table_name: &'static str,
        processor_name: &str,
        processor: for<'a> fn(
            &'a (RecoveredBlock<Block>, Vec<Receipt>),
            ProcessingComponents<Node>,
            &'a RocksDB
        ) -> futures::future::BoxFuture<'a, Result<()>>,
    ) -> Self {
        Self {
            table_name,
            processor_name: processor_name.to_string(),
            processor,
        }
    }
}

pub struct Liquidator <Node: FullNodeComponents> {
    processors: Vec<ProcessorInfo<Node>>,
    config: StrategyConfig,
}

impl<Node: FullNodeComponents> Strategy for Liquidator<Node>
where
    Node::Types: NodeTypes,
    <Node::Types as NodeTypes>::Primitives: NodePrimitives<
    BlockHeader = Header,
    Block = Block<TransactionSigned>,
    Receipt = Receipt,
    SignedTx = TransactionSigned,
    >,
{
    // TODO : define action for liquidator
    type Action = LiquidatorTodoAction;

    fn new(config: StrategyConfig) -> Self {
        Self { processors: Vec::new(), config: config.clone() }
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
        let config_result = self.config.load_candidates(chain_id);
        let processors = match config_result {
            StrategyCandidates::Processors(c) => c,
            _ => vec![],
        };
        // processors for 문 돌면서 add_processor 해줘야함
        for processor in processors {
            self.add_processor(processor.table, processor.processor);
        }

        tracing::info!("Initialized liquidator with processors: {:?}", self.list_processors());
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
        Ok(None)
    }
}


impl<Node: FullNodeComponents> Liquidator<Node>
where
    Node::Types: NodeTypes,
    <Node::Types as NodeTypes>::Primitives: NodePrimitives<
    BlockHeader = Header,
    Block = Block<TransactionSigned>,
    Receipt = Receipt,
    SignedTx = TransactionSigned,
    >,
    Node::Provider: reth::providers::BlockReader<Block = Block<TransactionSigned>>
    + reth::providers::HeaderProvider<Header = Header>
    + reth::providers::ReceiptProvider<Receipt = Receipt>
    + reth::providers::TransactionsProvider<Transaction = TransactionSigned>,
    Node::Evm: ConfigureEvm
{
    // add_processor adds indexer processor to liquidator
    fn add_processor(&mut self, table_name: String, processor_name: String) {
        let table = match TableName::from_str(&table_name) {
            Some(t) => t,
            None => return, // Skip if table name is not recognized
        };

        let processor = match table {
            TableName::DolomiteBorrowPositions => ProcessorInfo::new(
                table.as_str(),
                &processor_name,
                |block_data, components, db| Box::pin(process_dolomite_borrow_positions::<Node>(block_data, components, db))
            ),
            TableName::AaveExecuteBorrow => ProcessorInfo::new(
                table.as_str(),
                &processor_name,
                |block_data, components, db| Box::pin(process_aave_execute_borrow::<Node>(block_data, components, db))
            ),
        };
        self.processors.push(processor);
    }

    fn list_processors(&self) -> Vec<&str> {
        self.processors.iter().map(|p| p.processor_name.as_str()).collect()
    }

    async fn process_blocks(
        &self,
        blocks_and_receipts: impl Iterator<Item = (&RecoveredBlock<Block>, &Vec<Receipt>)>,
        db: &RocksDB,
        provider: Node::Provider,
    ) -> Result<()>
    where
        Node::Evm: ConfigureEvm
    {
        // Convert the iterator items into owned values directly
        let blocks_and_receipts: Vec<_> = blocks_and_receipts
            .map(|(block, receipts)| (block.clone(), receipts.clone()))
            .collect();

        for (block, receipts) in blocks_and_receipts {
            let block_number = block.number;
            
            // Create components for processing
            let components = ProcessingComponents {
                provider: provider.clone(),
                config: self.config.clone(),
            };

            let block_data = (block, receipts);
            if let Err(e) = self.process_block_data(&block_data, components, db).await {
                tracing::warn!("Failed to process block {}: {}", block_number, e);
            }
        }

        Ok(())
    }

    async fn process_block_data(
        &self,
        block_data: &(RecoveredBlock<Block>, Vec<Receipt>),
        components: ProcessingComponents<Node>,
        db: &RocksDB
    ) -> Result<()>
    where
        Node::Types: NodeTypes,
        <Node::Types as NodeTypes>::Primitives: NodePrimitives<
            BlockHeader = Header,
            Block = Block<TransactionSigned>,
            Receipt = Receipt,
            SignedTx = TransactionSigned
        >,
    {
        let block_number = block_data.0.number;

        // Create a vector to store all processing tasks
        let mut tasks = Vec::new();

        // Spawn a task for each enabled processor
        for processor in &self.processors {
            // Clone necessary data for the task
            let processor_name = processor.processor_name.clone();
            let processor_fn = processor.processor;
            let block_data = block_data.clone();
            let components = components.clone();
            let db = db.clone();  // Clone db before spawn
            
            // Spawn the task
            let task = tokio::spawn(async move {
                let event_start_time = Instant::now();
                match processor_fn(&block_data, components, &db).await {
                    Ok(()) => Ok((processor_name, event_start_time.elapsed())),
                    Err(e) => Err((processor_name, e.to_string()))
                }
            });

            tasks.push(task);
        }

        // Wait for all tasks to complete and collect results
        let mut total_records = 0;
        let mut event_results = Vec::new();
        let mut failed_events = Vec::new();

        for task in tasks {
            match task.await {
                Ok(Ok((name, duration))) => {
                    total_records += 1; // Assuming each event writes one record for now
                    event_results.push((name, duration));
                }
                Ok(Err((name, error))) => {
                    failed_events.push((name, error));
                }
                Err(e) => {
                    tracing::warn!("Task join error: {}", e);
                }
            }
        }

        // Sort events by name for consistent logging
        event_results.sort_by(|a, b| a.0.cmp(&b.0));

        // Create a consolidated success log
        if !event_results.is_empty() {
            let events_summary: Vec<String> = event_results
                .iter()
                .map(|(name, time)| {
                    format!("{}({}, {:.2}s)", name, 1, time.as_secs_f64())
                })
                .collect();

            tracing::info!(
                "exex{{id=\"exex-indexer\"}}: Block {} processed - Events: [{}], Total records: {}",
                block_number,
                events_summary.join(", "),
                total_records,
            );
        }

        // Create a consolidated error log
        if !failed_events.is_empty() {
            let failure_summary: Vec<String> = failed_events
                .iter()
                .map(|(name, error)| format!("{}: {}", name, error))
                .collect();

            tracing::warn!(
                "exex{{id=\"exex-indexer\"}}: Block {} failures - {}",
                block_number,
                failure_summary.join(", ")
            );
        }

        Ok(())
    }
}