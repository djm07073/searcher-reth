pub mod model;
mod schema;
mod types;

use diesel::{prelude::*, sqlite::SqliteConnection};
use model::{Hop as HopModel, HopType};
use reth_revm::primitives::Address;
use searcher_reth_strategy::{Hop, core::candidate::CandidatesResult};
use std::collections::HashMap;

impl From<HopModel> for Hop {
    fn from(val: HopModel) -> Self {
        Hop {
            dexType: val.dex_type as u8,
            dex: val.address.parse::<Address>().unwrap(),
            srcToken: val.src_token.parse::<Address>().unwrap(),
            dstToken: val.dst_token.parse::<Address>().unwrap(),
            metadata: val.metadata.into_bytes().into(),
        }
    }
}

pub struct SearcherRepository {
    database_url: String,
}

impl SearcherRepository {
    pub fn new(database_url: &str) -> Self {
        Self { database_url: database_url.into() }
    }

    pub fn get_candidates(&self, id: u64) -> CandidatesResult<Hop> {
        use schema::hop::dsl::*;

        let mut conn = SqliteConnection::establish(&self.database_url)?;

        let start_hops: Vec<HopModel> = hop
            .filter(chain_id.eq(id as i32))
            .filter(hop_type.eq(HopType::Start as i32))
            .load(&mut conn)?;

        let inter_hops: Vec<HopModel> = hop
            .filter(chain_id.eq(id as i32))
            .filter(hop_type.eq(HopType::Inter as i32))
            .load(&mut conn)?;

        let end_hops: Vec<HopModel> = hop
            .filter(chain_id.eq(id as i32))
            .filter(hop_type.eq(HopType::End as i32))
            .load(&mut conn)?;

        let mut path_map: HashMap<Address, Vec<Vec<Hop>>> = HashMap::new();

        for start in &start_hops {
            let start_token: Address = start.src_token.parse().unwrap();
            let mut paths = Vec::new();

            // 2-hop paths
            for end in &end_hops {
                if start.dst_token == end.src_token {
                    paths.push(vec![start.clone().into(), end.clone().into()]);
                }
            }

            // 3-hop paths
            for inter in &inter_hops {
                if start.dst_token == inter.src_token {
                    for end in &end_hops {
                        if inter.dst_token == end.src_token {
                            paths.push(vec![
                                start.clone().into(),
                                inter.clone().into(),
                                end.clone().into(),
                            ]);
                        }
                    }
                }
            }

            if !paths.is_empty() {
                path_map.insert(start_token, paths);
            }
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
