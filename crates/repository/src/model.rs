use crate::schema::*;
use diesel::prelude::*;

#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum HopType {
    Start = 0,
    Inter = 1,
    End = 2,
}

#[derive(Debug, Clone, Queryable, Identifiable)]
#[diesel(primary_key(chain_id, address, dex_type, src_token, dst_token))]
#[diesel(table_name = hop)]
pub struct Hop {
    pub chain_id: i32,
    pub address: String,
    pub dex_type: i32,
    pub src_token: String,
    pub dst_token: String,
    pub hop_type: i32,
    pub metadata: String,
}
