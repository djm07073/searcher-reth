use crate::utils::Config;
use crate::db_writer::RocksDB;
use crate::datasets::{
    dolomite_borrow_position::process_dolomite_borrow_positions,
    aave_execute_borrow::process_aave_execute_borrow,
};
use alloy_consensus::{Header};
use alloy_rpc_types::{BlockId, BlockNumberOrTag};
use eyre::Result;
use reth::builder::NodeTypes;
use reth::primitives::{EthereumHardforks, NodePrimitives};
use reth_primitives::{TransactionSigned, Receipt, RecoveredBlock, Block};
use reth_node_api::{ConfigureEvm, FullNodeComponents, FullNodeTypes};
use reth_tracing::tracing::{info, warn};
use std::{sync::Arc, time::Instant, collections::HashSet};

// Structure to hold all the components needed for processing
#[derive(Clone)]
pub struct ProcessingComponents<Node: FullNodeComponents> {
    // pub eth_api: Arc<EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>>,
    // pub block_traces: Option<Vec<TraceResultsWithTransactionHash>>,
    pub provider: Node::Provider,
    //pub client: Arc<Client>,
    pub config: Config,
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

pub struct Indexer<Node: FullNodeComponents> {
    processors: Vec<ProcessorInfo<Node>>,
    config: Config,
}

impl<Node> Indexer<Node>
where
    Node: FullNodeComponents + FullNodeTypes,
    Node::Types: NodeTypes,
    //<Node::Types as NodeTypes>::ChainSpec: EthereumHardforks,
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
    //EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>: Call + LoadPendingBlock + FullEthApiTypes,
{
    pub fn new(config: Config) -> Self {
        let mut indexer = Self {
            processors: Vec::new(),
            config,
        };

        // Register all available processors
        // e.g) indexer.add_processor("headers", "Headers");
        indexer.add_processor("dolomite_borrow_positions", "DolomiteBorrowPositions");
        //indexer.add_processor("aave_borrow", "AaveBorrow");

        info!("Initialized indexer with processors: {:?}", indexer.list_processors());
        indexer
    }

    pub fn add_processor(&mut self, table_name: &'static str, processor_name: &'static str) {
        let processor = match table_name {
            "dolomite_borrow_positions" => ProcessorInfo::new(
                table_name,
                processor_name,
                |block_data, components, db| Box::pin(process_dolomite_borrow_positions::<Node>(block_data, components, db))
            ),
            // "aave_borrow" => ProcessorInfo::new(
            //     table_name,
            //     processor_name,
            //     |block_data, components, db| Box::pin(process_aave_execute_borrow::<Node>(block_data, components, db))
            // ),
            _ => return, // Skip unknown processors
        };
        self.processors.push(processor);
    }

    pub fn list_processors(&self) -> Vec<&str> {
        self.processors.iter().map(|p| p.processor_name).collect()
    }

    pub async fn process_blocks(
        &self,
        blocks_and_receipts: impl Iterator<Item = (&RecoveredBlock<Block>, &Vec<Receipt>)>,
        //client: &Arc<Client>,
        db: &RocksDB,
        provider: Node::Provider,
        //evm_config: Arc<Node::Evm>,
        //pool: Arc<Node::Pool>,
        //network: Arc<Node::Network>,
    ) -> Result<()>
    where
        Node::Evm: ConfigureEvm
        //EthApi<Node::Provider, Node::Pool, Node::Network, Node::Evm>: Call + LoadPendingBlock + FullEthApiTypes,
    {
        // Convert the iterator items into owned values directly
        let blocks_and_receipts: Vec<_> = blocks_and_receipts
            .map(|(block, receipts)| (block.clone(), receipts.clone()))
            .collect();

        for (block, receipts) in blocks_and_receipts {
            let block_number = block.number;
            //let block_id = BlockId::Number(BlockNumberOrTag::from(block_number));

            // Create EthAPI
            // let eth_api = crate::utils::create_eth_api::<Node>(
            //     provider.clone(),
            //     (*evm_config).clone(),
            //     (*pool).clone(),
            //     (*network).clone()
            // );

            // // Create TraceAPI
            // let trace_api = crate::utils::create_trace_api::<Node>(
            //     provider.clone(),
            //     (*evm_config).clone(),
            //     (*pool).clone(),
            //     (*network).clone()
            // );

            // // Get traces once for the block
            // let block_traces = match trace_api.replay_block_transactions(
            //     block_id,
            //     HashSet::from_iter(vec![TraceType::Trace])
            // ).await {
            //     Ok(traces) => traces,
            //     Err(e) => {
            //         warn!("Failed to get traces for block {}: {}", block_number, e);
            //         None
            //     }
            // };

            // Create components for processing
            let components = ProcessingComponents {
                // eth_api: eth_api.clone(),
                // block_traces,
                provider: provider.clone(),
                //client: Arc::clone(client),
                config: self.config.clone(),
            };

            let block_data = (block, receipts);
            if let Err(e) = self.process_block_data(&block_data, components, db).await {
                warn!("Failed to process block {}: {}", block_number, e);
            }
        }

        Ok(())
    }

    pub async fn process_block_data(
        &self,
        block_data: &(RecoveredBlock<Block>, Vec<Receipt>),
        components: ProcessingComponents<Node>,
        db: &RocksDB
    ) -> Result<()>
    where
        Node::Types: NodeTypes,
        //<Node::Types as NodeTypes>::ChainSpec: EthereumHardforks,
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
            if self.config.is_event_enabled(processor.processor_name) {
                // Clone necessary data for the task
                let processor_name = processor.processor_name;
                let processor_fn = processor.processor;
                let block_data = block_data.clone();
                let components = components.clone();
                let db = db.clone();  // Clone db before spawn
                // let table = get_table(processor.table_name)
                //     .expect(&format!("Table definition not found for {}", processor.table_name));

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
                    warn!("Task join error: {}", e);
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

            info!(
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

            warn!(
                "exex{{id=\"exex-indexer\"}}: Block {} failures - {}",
                block_number,
                failure_summary.join(", ")
            );
        }

        Ok(())
    }
}