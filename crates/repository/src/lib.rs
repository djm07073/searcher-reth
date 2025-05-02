// ! update tokens and dexs
mod orm;
pub mod types;

use eyre::Result;
use reth_revm::primitives::Address;
use searcher_reth_path_finder::DexType;
use sqlx::{ postgres::PgPoolOptions, PgPool, Row, Transaction };
use types::Priority;

pub struct SearcherRepository {
    pool: PgPool,
}

impl SearcherRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(database_url).await?;
        Ok(Self { pool })
    }

    pub async fn get_all_tokens(&self, chain_id: u64) -> Result<Vec<(Address, Priority)>> {
        let rows = sqlx
            ::query(
                "SELECT address, priority, chain_id FROM tokens WHERE chain_id = $1 ORDER BY priority DESC"
            )
            .bind(chain_id as i64)
            .fetch_all(&self.pool).await?;

        let tokens = rows
            .into_iter()
            .map(|row| {
                let addr: Address = row.get::<String, _>("address").parse().unwrap();
                let priority = row.get::<i64, _>("priority");
                (addr, Priority::from(priority))
            })
            .collect();

        Ok(tokens)
    }

    pub async fn get_all_dexs(&self, chain_id: u64) -> Result<Vec<(Address, DexType)>> {
        let rows = sqlx
            ::query("SELECT address, dex_type FROM dexs WHERE chain_id = $1")
            .bind(chain_id as i64)
            .fetch_all(&self.pool).await?;

        let dexs = rows
            .into_iter()
            .map(|row| {
                let addr: Address = row.get::<String, _>("address").parse().unwrap();
                let dex_type = row.get::<i64, _>("dex_type");
                (addr, dex_type as DexType)
            })
            .collect();

        Ok(dexs)
    }

    pub async fn update_route_paths(
        &self,
        chain_id: u64,
        new_tokens: &Option<Vec<(Address, u64)>>,
        deprecated_tokens: &Option<Vec<Address>>,
        new_dexs: &Option<Vec<(DexType, Address)>>,
        deprecated_dexs: &Option<Vec<Address>>
    ) -> Result<()> {
        let mut txn = self.pool.begin().await?;

        self.insert_tokens(&mut txn, chain_id, new_tokens).await?;
        self.delete_tokens(&mut txn, chain_id, deprecated_tokens).await?;
        self.insert_dexs(&mut txn, chain_id, new_dexs).await?;
        self.delete_dexs(&mut txn, chain_id, deprecated_dexs).await?;

        txn.commit().await?;

        Ok(())
    }

    async fn insert_tokens(
        &self,
        txn: &mut Transaction<'_, sqlx::Postgres>,
        chain_id: u64,
        tokens: &Option<Vec<(Address, u64)>>
    ) -> Result<()> {
        if let Some(tokens) = tokens {
            for (address, priority) in tokens {
                sqlx
                    ::query(
                        "INSERT INTO tokens (chain_id, address, priority)
                     VALUES ($1, $2, $3)
                     ON CONFLICT (chain_id, address) DO UPDATE SET priority = $3"
                    )
                    .bind(chain_id as i64)
                    .bind(address.to_string())
                    .bind(*priority as i64)
                    .execute(&mut **txn).await?;
            }
        }
        Ok(())
    }

    async fn insert_dexs(
        &self,
        txn: &mut Transaction<'_, sqlx::Postgres>,
        chain_id: u64,
        dexs: &Option<Vec<(DexType, Address)>>
    ) -> Result<()> {
        if let Some(dexs) = dexs {
            for (dex_type, address) in dexs {
                sqlx
                    ::query(
                        "INSERT INTO dexs (chain_id, address, dex_type)
                     VALUES ($1, $2, $3)
                     ON CONFLICT (chain_id, address) DO UPDATE SET dex_type = $3"
                    )
                    .bind(chain_id as i64)
                    .bind(address.to_string())
                    .bind(*dex_type as i64)
                    .execute(&mut **txn).await?;
            }
        }
        Ok(())
    }

    async fn delete_tokens(
        &self,
        txn: &mut Transaction<'_, sqlx::Postgres>,
        chain_id: u64,
        tokens: &Option<Vec<Address>>
    ) -> Result<()> {
        if let Some(tokens) = tokens {
            for address in tokens {
                sqlx
                    ::query("DELETE FROM tokens WHERE chain_id = $1 AND address = $2")
                    .bind(chain_id as i64)
                    .bind(address.to_string())
                    .execute(&mut **txn).await?;
            }
        }
        Ok(())
    }

    async fn delete_dexs(
        &self,
        txn: &mut Transaction<'_, sqlx::Postgres>,
        chain_id: u64,
        dexs: &Option<Vec<Address>>
    ) -> Result<()> {
        if let Some(dexs) = dexs {
            for address in dexs {
                sqlx
                    ::query("DELETE FROM dexs WHERE chain_id = $1 AND address = $2")
                    .bind(chain_id as i64)
                    .bind(address.to_string())
                    .execute(&mut **txn).await?;
            }
        }
        Ok(())
    }

    pub async fn insert_contract(&self, chain_id: u64, contract_code: String) -> Result<()> {
        sqlx
            ::query(
                "INSERT INTO contracts (chain_id, code) VALUES ($1, $2)
                     ON CONFLICT (chain_id) DO UPDATE SET code = $2"
            )
            .bind(chain_id as i64)
            .bind(contract_code)
            .execute(&self.pool).await?;

        Ok(())
    }

    pub async fn update_contract(&self, chain_id: u64, contract_code: String) -> Result<()> {
        sqlx
            ::query("UPDATE contracts SET code = $1 WHERE chain_id = $2")
            .bind(contract_code)
            .bind(chain_id as i64)
            .execute(&self.pool).await?;

        Ok(())
    }
}
