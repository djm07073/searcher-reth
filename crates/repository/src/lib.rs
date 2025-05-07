mod entity;
pub mod types;

use eyre::Result;
use reth_revm::primitives::Address;
use sea_orm::{ QueryOrder, TransactionTrait };
use sea_orm::{
    DatabaseConnection,
    Database,
    EntityTrait,
    ActiveModelTrait,
    ActiveValue::Set,
    QueryFilter,
    ColumnTrait,
};
use entity::prelude::*;
use entity::{ token, dex, contract };

use migration::{ Migrator, MigratorTrait };
use types::{DexType, Priority};

pub struct SearcherRepository {
    conn: DatabaseConnection,
}

impl SearcherRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let conn = Database::connect(database_url).await?;

        Migrator::up(&conn, None).await?;

        Ok(Self { conn })
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
                let dex_type = dex.dex_type.parse::<i64>().unwrap();
                (addr, dex_type as DexType)
            })
            .collect();

        Ok(result)
    }

    pub async fn update_route_paths(
        &self,
        chain_id: u64,
        new_tokens: &Option<Vec<(Address, i64)>>,
        deprecated_tokens: &Option<Vec<Address>>,
        new_dexs: &Option<Vec<(DexType, Address)>>,
        deprecated_dexs: &Option<Vec<Address>>
    ) -> Result<()> {
        let txn = self.conn.begin().await?;

        if let Some(tokens) = new_tokens {
            for (address, priority) in tokens {
                let token = token::ActiveModel {
                    chain_id: Set(chain_id as i64),
                    address: Set(address.to_string()),
                    priority: Set(*priority),
                };
                token.insert(&txn).await?;
            }
        }

        if let Some(tokens) = deprecated_tokens {
            for address in tokens {
                Token::delete_many()
                    .filter(
                        token::Column::ChainId
                            .eq(chain_id as i64)
                            .and(token::Column::Address.eq(address.to_string()))
                    )
                    .exec(&txn).await?;
            }
        }

        if let Some(dexs) = new_dexs {
            for (dex_type, address) in dexs {
                let dex = dex::ActiveModel {
                    chain_id: Set(chain_id as i64),
                    address: Set(address.to_string()),
                    dex_type: Set((*dex_type as i64).to_string()),
                };
                dex.insert(&txn).await?;
            }
        }

        if let Some(dexs) = deprecated_dexs {
            for address in dexs {
                Dex::delete_many()
                    .filter(
                        dex::Column::ChainId
                            .eq(chain_id as i64)
                            .and(dex::Column::Address.eq(address.to_string()))
                    )
                    .exec(&txn).await?;
            }
        }

        txn.commit().await?;
        Ok(())
    }

    pub async fn insert_contract(&self, chain_id: u64, contract_code: String) -> Result<()> {
        let contract = contract::ActiveModel {
            chain_id: Set(chain_id as i64),
            code: Set(contract_code),
        };
        contract.insert(&self.conn).await?;
        Ok(())
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
