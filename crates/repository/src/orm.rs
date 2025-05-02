use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct Contract {
    pub chain_id: i64,
    pub code: String,
}

#[derive(Debug, FromRow)]
pub struct Token {
    pub chain_id: i64,
    pub address: String,
    pub priority: i64,
}

#[derive(Debug, FromRow)]
pub struct Dex {
    pub chain_id: i64,
    pub address: String,
    pub dex_type: String,
}
