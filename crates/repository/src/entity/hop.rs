use sea_orm::entity::prelude::*;
use super::{ dex, token };

#[derive(Debug, Clone, PartialEq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "i32", db_type = "Integer")]
pub enum HopType {
    #[sea_orm(num_value = 0)]
    Start,
    #[sea_orm(num_value = 1)]
    Inter,
    #[sea_orm(num_value = 2)]
    End,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "hop")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub chain_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub address: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub dex_type: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub src_token: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub dst_token: String,
    #[sea_orm(auto_increment = false)]
    pub hop_type: HopType,
    #[sea_orm(auto_increment = false)]
    pub metadata: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "dex::Entity",
        from = "(Column::ChainId, Column::Address, Column::DexType)",
        to = "(dex::Column::ChainId, dex::Column::Address, dex::Column::DexType)"
    )]
    Dex,
    #[sea_orm(
        belongs_to = "token::Entity",
        from = "(Column::ChainId, Column::SrcToken)",
        to = "(token::Column::ChainId, token::Column::Address)"
    )]
    SourceToken,
    #[sea_orm(
        belongs_to = "token::Entity",
        from = "(Column::ChainId, Column::DstToken)",
        to = "(token::Column::ChainId, token::Column::Address)"
    )]
    DestinationToken,
}

impl ActiveModelBehavior for ActiveModel {}
