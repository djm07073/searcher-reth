use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hop {
    pub address: String,
    pub dex_type: u8,
    pub src_token: String,
    pub dst_token: String,
    pub metadata: String,
}

pub type Route = Vec<Hop>;
pub type Routes = Vec<Route>;