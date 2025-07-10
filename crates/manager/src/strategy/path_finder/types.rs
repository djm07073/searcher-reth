use alloy_primitives::map::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteElement {
    pub address: String,
    pub dex_type: u8,
    pub src_token: String,
    pub dst_token: String,
    pub metadata: String,
    pub encoded_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub initial_tokens: Vec<String>,
    pub elements: Vec<RouteElement>,
}

pub type RoutesMap = HashMap<String, Route>;
