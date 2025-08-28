use crate::liquidator::db_writer::RocksDB;
use alloy_primitives::{Address, address};
use alloy_sol_types::{SolEvent, sol};
use eyre::Result;
use reth_primitives::{Block, Receipt, RecoveredBlock};
use reth_tracing::tracing::debug;

// Pool contarct address
const ARBITRUM_AAVE_CONTRACT_ADDRESS: Address =
    address!("0x794a61358D6845594F94dc1DB02A252b5b4814aD");

sol! {
    event Borrow(
        address indexed reserve,
        address user,
        address indexed onBehalfOf,
        uint256 amount,
        uint8 interestRateMode,
        uint256 borrowRate,
        uint16 indexed referralCode
      );
}

pub async fn process_aave_execute_borrow(
    block_data: &(RecoveredBlock<Block>, Vec<Receipt>),
    db: &RocksDB,
) -> Result<()> {
    let block = &block_data.0;
    let receipts = &block_data.1;

    // Iterate through transactions and their receipts
    for (tx_idx, (tx, receipt)) in block.body().transactions.iter().zip(receipts.iter()).enumerate()
    {
        // Process each log in the receipt
        for (log_idx, log) in receipt.logs.iter().enumerate() {
            // Filter on dolomite contract address
            if log.address == ARBITRUM_AAVE_CONTRACT_ADDRESS {
                // First check if this log matches our event signature
                if log.topics().get(0) != Some(&Borrow::SIGNATURE_HASH) {
                    continue;
                }

                match Borrow::decode_raw_log(log.topics(), &log.data.data) {
                    Ok(create) => {
                        db.save(
                            "aave_borrow",
                            &format!("{}", create.onBehalfOf),
                            &format!("{}_{}", create.reserve, create.amount),
                        );
                    }
                    Err(e) => {
                        debug!("Failed to decode aave borrow event: {:?}", e);
                    }
                }
            }
        }
    }
    Ok(())
}
