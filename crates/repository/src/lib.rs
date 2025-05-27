pub mod entity;
pub mod types;

use entity::{ contract, dex, hop, prelude::*, token };
use eyre::Result;
use reth_revm::primitives::Address;
use std::collections::HashMap;
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait,
    Database,
    DatabaseConnection,
    EntityTrait,
    QueryFilter,
    QueryOrder,
};

use migration::{ Migrator, MigratorTrait };
use types::{ DexType, Priority };

pub struct SearcherRepository {
    conn: DatabaseConnection,
}

impl SearcherRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let conn = Database::connect(database_url).await?;

        Migrator::up(&conn, None).await?;

        Ok(Self { conn })
    }

    pub async fn get_route_paths(
        &self,
        chain_id: u64
    ) -> Result<HashMap<Address, Vec<Vec<hop::Model>>>> {
        let start_hops = HopEntity::find()
            .filter(hop::Column::ChainId.eq(chain_id as i64))
            .filter(hop::Column::HopType.eq(hop::HopType::Start))
            .all(&self.conn).await?;

        let inter_hops = HopEntity::find()
            .filter(hop::Column::ChainId.eq(chain_id as i64))
            .filter(hop::Column::HopType.eq(hop::HopType::Inter))
            .all(&self.conn).await?;

        let end_hops = HopEntity::find()
            .filter(hop::Column::ChainId.eq(chain_id as i64))
            .filter(hop::Column::HopType.eq(hop::HopType::End))
            .all(&self.conn).await?;

        let mut path_map: HashMap<Address, Vec<Vec<hop::Model>>> = HashMap::new();

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

    pub async fn get_all_tokens(&self, chain_id: u64) -> Result<Vec<(Address, Priority)>> {
        let tokens = Token::find()
            .filter(token::Column::ChainId.eq(chain_id as i64))
            .order_by_asc(token::Column::Priority)
            .all(&self.conn).await?;

        let result = tokens
            .into_iter()
            .map(|token| {
                let addr: Address = token.address.parse().unwrap();
                (addr, token.priority.into())
            })
            .collect();

        Ok(result)
    }

    pub async fn get_all_dexs(&self, chain_id: u64) -> Result<Vec<(Address, DexType)>> {
        let dexs = Dex::find()
            .filter(dex::Column::ChainId.eq(chain_id as i64))
            .all(&self.conn).await?;

        let result = dexs
            .into_iter()
            .map(|dex| {
                let addr: Address = dex.address.parse().unwrap();
                let dex_type = dex.dex_type;
                (addr, dex_type as DexType)
            })
            .collect();

        Ok(result)
    }

    pub async fn update_contract(&self, chain_id: u64, contract_code: String) -> Result<()> {
        let contract = contract::ActiveModel {
            chain_id: Set(chain_id as i64),
            code: Set(contract_code),
        };
        Contract::update(contract)
            .filter(contract::Column::ChainId.eq(chain_id as i64))
            .exec(&self.conn).await?;
        Ok(())
    }
}
