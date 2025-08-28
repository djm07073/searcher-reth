pub type Element = Vec<u8>;

pub type Candidate = Vec<Element>;

use alloy_primitives::{Address, hex};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

fn deserialize_hex_string<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let hex_str: String = Deserialize::deserialize(deserializer)?;
    let hex_str = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
    hex::decode(hex_str).map_err(serde::de::Error::custom)
}

fn deserialize_address<'de, D>(deserializer: D) -> Result<Address, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    let s = s.strip_prefix("0x").unwrap_or(&s);
    let bytes = hex::decode(s).map_err(serde::de::Error::custom)?;
    if bytes.len() != 20 {
        return Err(serde::de::Error::custom("Address must be 20 bytes"));
    }
    Ok(Address::from_slice(&bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateEntry {
    #[serde(deserialize_with = "deserialize_hex_string")]
    pub encoded: Vec<u8>,
    #[serde(deserialize_with = "deserialize_address")]
    pub initial_token: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorEntry {
    pub table: String,
    pub processor: String,
}

#[derive(Debug, Clone)]
pub enum StrategyCandidates {
    Candidates(Vec<CandidateEntry>),
    Processors(Vec<ProcessorEntry>),
}

pub type CandidateMap = HashMap<String, Vec<CandidateEntry>>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_deserialize_candidate_entry() {
        let json = r#"{
            "initial_token": "0x1111111111111111111111111111111111111111",
            "encoded": "0xdeadbeef"
        }"#;
        let entry: CandidateEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.initial_token, Address::from_slice(&[0x11u8; 20]));
        assert_eq!(entry.encoded, vec![0xde, 0xad, 0xbe, 0xef]);

        let json = r#"{
            "initial_token": "2222222222222222222222222222222222222222",
            "encoded": "cafebabe"
        }"#;
        let entry: CandidateEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.initial_token, Address::from_slice(&[0x22u8; 20]));
        assert_eq!(entry.encoded, vec![0xca, 0xfe, 0xba, 0xbe]);

        let json = r#"{
            "initial_token": "0x1234",
            "encoded": "0x00"
        }"#;
        let result: Result<CandidateEntry, _> = serde_json::from_str(json);
        assert!(result.is_err());

        let json = r#"{
            "initial_token": "0x1111111111111111111111111111111111111111",
            "encoded": "0xzzzz"
        }"#;
        let result: Result<CandidateEntry, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }
}
