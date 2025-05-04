use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "dex")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub chain_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub address: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub dex_type: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
