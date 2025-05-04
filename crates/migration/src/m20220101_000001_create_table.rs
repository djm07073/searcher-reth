use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum Token {
    Table,
    ChainId,
    Address,
    Priority,
}

#[derive(DeriveIden)]
enum Dex {
    Table,
    ChainId,
    Address,
    DexType,
}

#[derive(DeriveIden)]
enum Contract {
    Table,
    ChainId,
    Code,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Token table
        manager.create_table(
            Table::create()
                .table(Token::Table)
                .if_not_exists()
                .col(ColumnDef::new(Token::ChainId).integer().not_null())
                .col(ColumnDef::new(Token::Address).string().not_null())
                .col(ColumnDef::new(Token::Priority).integer().not_null())
                .primary_key(Index::create().col(Token::ChainId).col(Token::Address))
                .to_owned()
        ).await?;

        // Create Dex table
        manager.create_table(
            Table::create()
                .table(Dex::Table)
                .if_not_exists()
                .col(ColumnDef::new(Dex::ChainId).integer().not_null())
                .col(ColumnDef::new(Dex::Address).string().not_null())
                .col(ColumnDef::new(Dex::DexType).string().not_null())
                .primary_key(Index::create().col(Dex::ChainId).col(Dex::Address))
                .to_owned()
        ).await?;

        // Create Contract table
        manager.create_table(
            Table::create()
                .table(Contract::Table)
                .if_not_exists()
                .col(ColumnDef::new(Contract::ChainId).integer().not_null())
                .col(ColumnDef::new(Contract::Code).string().not_null())
                .primary_key(Index::create().col(Contract::ChainId))
                .to_owned()
        ).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop tables in reverse order
        manager.drop_table(Table::drop().table(Contract::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(Dex::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(Token::Table).to_owned()).await?;

        Ok(())
    }
}
