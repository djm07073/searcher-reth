use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

use alloy_network::EthereumWallet;
use alloy_primitives::{Address, map::foldhash::HashMap};

pub struct RelayerWallet {
    wallet: EthereumWallet,
    idx: AtomicUsize,
    signers: Vec<(Address, Arc<AtomicU64>)>,
}

impl RelayerWallet {
    pub fn new(
        wallet: (EthereumWallet, Vec<Address>),
        get_nonce_fn: impl Fn(Address) -> Arc<AtomicU64>,
    ) -> Self {
        let (wallet, addresses) = wallet;
        let signers =
            addresses.into_iter().map(|address| (address, get_nonce_fn(address))).collect();
        Self { wallet, idx: AtomicUsize::new(0), signers }
    }

    pub fn next_signer(&self) -> (Address, Arc<AtomicU64>) {
        let idx = self.idx.fetch_add(1, Ordering::AcqRel);
        let (address, atomic_nonce) = self.signers[idx % self.signers.len()].clone();
        (address, atomic_nonce)
    }
}
