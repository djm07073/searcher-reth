use crate::db_writer::RocksDB;
use alloy_sol_types::{sol, SolEvent};
use alloy_primitives::{address, Address};
use reth_primitives::{RecoveredBlock, Receipt, Block};
use eyre::Result;
use reth_node_api::FullNodeComponents;
use tracing::debug;
use crate::indexer::ProcessingComponents;
// BorrowPositionProxyV2 address
const DOLOMITE_CONTRACT_ADDRESS: Address = address!("0xC06271eb97d960F4034DDF953e16271CcB2B10BD");

sol! {
    event BorrowPositionOpen(
        address indexed _borrower,
        uint256 indexed _borrowAccountNumber
    );
}

pub async fn process_dolomite_borrow_positions<Node: FullNodeComponents>(
    block_data: &(RecoveredBlock<Block>, Vec<Receipt>),
    components: ProcessingComponents<Node>,
    db: &RocksDB,
) -> Result<()> {
    let block = &block_data.0;
    let receipts = &block_data.1;

    // Iterate through transactions and their receipts
    for (tx_idx, (tx, receipt)) in block.body().transactions.iter().zip(receipts.iter()).enumerate() {
        // Process each log in the receipt
        for (log_idx, log) in receipt.logs.iter().enumerate() {
            // Filter on dolomite contract address
            if log.address == DOLOMITE_CONTRACT_ADDRESS {
                // First check if this log matches our event signature
                if log.topics().get(0) != Some(&BorrowPositionOpen::SIGNATURE_HASH) {
                    continue;
                }

                match BorrowPositionOpen::decode_raw_log(log.topics(), &log.data.data) {
                    Ok(create) => {
                        db.save(
                            "dolomite_borrow_positions",
                            &format!("{}", create._borrowAccountNumber),
                            &format!("{}", create._borrower)
                        );
                    }
                    Err(e) => {
                        debug!("Failed to decode dolomite borrow position open event: {:?}", e);
                    }
                }
            }
        }
    }
    Ok(())
}

