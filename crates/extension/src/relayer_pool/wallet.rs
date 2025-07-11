use std::sync::atomic::{AtomicUsize, Ordering};

use alloy_network::EthereumWallet;
use alloy_primitives::Address;

pub struct WalletPool {
    wallet: EthereumWallet,
    idx: AtomicUsize,
    signers: Vec<Address>,
}

impl WalletPool {
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

    pub fn signers(&self) -> &[Address] {
        &self.signers
    }
}
