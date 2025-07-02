pub mod model;
mod types;

use eyre::{eyre, Result};
use model::{Hop as HopModel, Routes};
use reth_revm::primitives::Address;
use searcher_reth_strategy::{Hop, core::candidate::CandidatesResult};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

impl From<HopModel> for Hop {
    fn from(val: HopModel) -> Self {
        Hop {
            dexType: val.dex_type,
            dex: val.address.parse::<Address>().unwrap(),
            srcToken: val.src_token.parse::<Address>().unwrap(),
            dstToken: val.dst_token.parse::<Address>().unwrap(),
            metadata: hex::decode(&val.metadata.trim_start_matches("0x")).unwrap().into(),
        }
    }
}

pub struct SearcherRepository {
    routes: Routes,
}

impl SearcherRepository {
    pub fn new(json_path: &str) -> Result<Self> {
        let path = Path::new(json_path);
        if !path.exists() {
            return Err(eyre!("Routes JSON file not found at: {}", json_path));
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let routes: Routes = serde_json::from_reader(reader)
            .map_err(|e| eyre!("Failed to parse routes JSON: {}", e))?;

        Ok(Self { routes })
    }

    pub fn get_candidates(&self, _chain_id: u64) -> CandidatesResult<Hop> {
        let mut path_map: HashMap<Address, Vec<Vec<Hop>>> = HashMap::new();

        for route in &self.routes {
            if route.is_empty() {
                continue;
            }

            // Get the starting token from the first hop
            let start_token: Address = route[0].src_token.parse().unwrap();
            
            // Convert route to Hop vec
            let hop_route: Vec<Hop> = route.iter()
                .map(|hop| hop.clone().into())
                .collect();

            // Group by starting token
            path_map.entry(start_token)
                .or_insert_with(Vec::new)
                .push(hop_route);
        }

        Ok(path_map)
    }
}

// re-export the types for external use
pub mod core {
    pub use searcher_reth_strategy::core::*;
}

pub mod config {
    pub use searcher_reth_strategy::config::*;
}

pub mod path_finding {
    pub use searcher_reth_strategy::{Hop, PathFinder};
}