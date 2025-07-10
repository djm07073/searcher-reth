use std::sync::atomic::{AtomicUsize, Ordering};

use alloy_network::EthereumWallet;
use alloy_primitives::Address;

pub struct RelayerWallet {
    wallet: EthereumWallet,
    idx: AtomicUsize,
    signers: Vec<Address>,
}

impl RelayerWallet {
    pub fn new(wallet: (EthereumWallet, Vec<Address>)) -> Self {
        let (wallet, signers) = wallet;
        Self { wallet, idx: AtomicUsize::new(0), signers }
    }

    pub fn next_signer(&self) -> Address {
        let idx = self.idx.fetch_add(1, Ordering::AcqRel);
        self.signers[idx % self.signers.len()]
    }

    pub fn wallet(&self) -> &EthereumWallet {
        &self.wallet
    }
}
