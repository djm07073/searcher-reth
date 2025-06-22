use std::collections::HashMap;

use alloy_primitives::Address;
use alloy_sol_types::SolStruct;
use eyre::Result;

pub type CandidatesResult<T> = Result<HashMap<Address, Vec<Vec<T>>>>;

pub trait Candidate<Config> {
    type Action: SolStruct + Clone;

    fn get_candidates(&self, chain_id: u64, config: &Config) -> CandidatesResult<Self::Action>;
}
