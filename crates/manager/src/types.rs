pub type Element = Vec<u8>;

pub type Candidate = Vec<Element>;

use serde::{ Deserialize, Serialize, Deserializer };
use std::collections::HashMap;
use alloy_primitives::hex;

fn deserialize_hex_string<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where D: Deserializer<'de>
{
    let hex_str: String = Deserialize::deserialize(deserializer)?;
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
    hex::decode(hex_str).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalldataCandidate {
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub quoter_calldata: Vec<u8>,
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub executor_calldata: Vec<u8>,
}

pub type RoutesMap = HashMap<String, Vec<CalldataCandidate>>;
