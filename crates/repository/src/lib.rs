pub mod model;
mod schema;
mod types;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use eyre::Result;
use reth_revm::primitives::Address;
use std::collections::HashMap;

use model::*;
use types::{ DexType, Priority };

pub struct SearcherRepository {
    database_url: String,
}

impl SearcherRepository {
    pub fn new(database_url: &str) -> Self {
        Self { database_url: database_url.into() }
    }

    pub fn get_route_paths(&self, id: u64) -> Result<HashMap<Address, Vec<Vec<Hop>>>> {
        use schema::hop::dsl::*;

        let mut conn = SqliteConnection::establish(&self.database_url)?;
        let start_hops: Vec<Hop> = hop
            .filter(chain_id.eq(id as i32))
            .filter(hop_type.eq(HopType::Start as i32)) // Start type
            .load(&mut conn)?;

        let inter_hops: Vec<Hop> = hop
            .filter(chain_id.eq(id as i32))
            .filter(hop_type.eq(HopType::Inter as i32)) // Inter type
            .load(&mut conn)?;

        let end_hops: Vec<Hop> = hop
            .filter(chain_id.eq(id as i32))
            .filter(hop_type.eq(HopType::End as i32)) // End type
            .load(&mut conn)?;

        let mut path_map: HashMap<Address, Vec<Vec<Hop>>> = HashMap::new();

        for start in &start_hops {
            let start_token: Address = start.src_token.parse().unwrap();
            let mut paths = Vec::new();

            for end in &end_hops {
                if start.dst_token == end.src_token {
                    paths.push(vec![start.clone(), end.clone()]);
                }
            }

            for inter in &inter_hops {
                if start.dst_token == inter.src_token {
                    for end in &end_hops {
                        if inter.dst_token == end.src_token {
                            paths.push(vec![start.clone(), inter.clone(), end.clone()]);
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

    pub fn get_all_tokens(&self, id: u64) -> Result<Vec<(Address, Priority)>> {
        use schema::token::dsl::*;
        let mut conn = SqliteConnection::establish(&self.database_url)?;
        let token_records = token
            .filter(chain_id.eq(id as i32))
            .order(priority.asc())
            .load::<Token>(&mut conn)?;

        let result = token_records
            .into_iter()
            .map(|t| {
                let addr: Address = t.address.parse().unwrap();
                (addr, t.priority.into())
            })
            .collect();

        Ok(result)
    }

    pub fn get_all_dexs(&self, id: u64) -> Result<Vec<(Address, DexType)>> {
        use schema::dex::dsl::*;
        let mut conn = SqliteConnection::establish(&self.database_url)?;
        let dex_records = dex.filter(chain_id.eq(id as i32)).load::<Dex>(&mut conn)?;

        let result = dex_records
            .into_iter()
            .map(|d| {
                let addr: Address = d.address.parse().unwrap();
                (addr, d.dex_type as DexType)
            })
            .collect();

        Ok(result)
    }

    pub fn update_contract(&self, id: u64, contract_code: String) -> Result<()> {
        use schema::contract::dsl::*;
        let mut conn = SqliteConnection::establish(&self.database_url)?;
        diesel
            ::update(contract.filter(chain_id.eq(id as i32)))
            .set(bytecode.eq(contract_code))
            .execute(&mut conn)?;

        Ok(())
    }
}
