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

#[derive(Iden)]
enum Hop {
    Table,
    ChainId,
    Address,
    DexType,
    SrcToken,
    DstToken,
    HopType,
    Metadata,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create Token table
        manager.create_table(
            Table::create()
                .table(Token::Table)
                .if_not_exists()
                .col(ColumnDef::new(Token::ChainId).big_integer().not_null())
                .col(ColumnDef::new(Token::Address).string().not_null())
                .col(ColumnDef::new(Token::Priority).big_integer().not_null())
                .primary_key(Index::create().col(Token::ChainId).col(Token::Address))
                .to_owned()
        ).await?;

        // Create Dex table
        manager.create_table(
            Table::create()
                .table(Dex::Table)
                .if_not_exists()
                .col(ColumnDef::new(Dex::ChainId).big_integer().not_null())
                .col(ColumnDef::new(Dex::Address).string().not_null())
                .col(ColumnDef::new(Dex::DexType).big_integer().not_null())
                .primary_key(Index::create().col(Dex::ChainId).col(Dex::Address).col(Dex::DexType))
                .to_owned()
        ).await?;

        // Create Hop table
        manager.create_table(
            Table::create()
                .table(Hop::Table)
                .if_not_exists()
                .col(ColumnDef::new(Hop::ChainId).big_integer().not_null())
                .col(ColumnDef::new(Hop::Address).string().not_null())
                .col(ColumnDef::new(Hop::DexType).big_integer().not_null())
                .col(ColumnDef::new(Hop::SrcToken).string().not_null())
                .col(ColumnDef::new(Hop::DstToken).string().not_null())
                .col(ColumnDef::new(Hop::HopType).integer().not_null())
                .col(ColumnDef::new(Hop::Metadata).string().not_null())
                .primary_key(
                    Index::create()
                        .col(Hop::ChainId)
                        .col(Hop::Address)
                        .col(Hop::DexType)
                        .col(Hop::SrcToken)
                        .col(Hop::DstToken)
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Hop::Table, (Hop::ChainId, Hop::Address, Hop::DexType))
                        .to(Dex::Table, (Dex::ChainId, Dex::Address, Dex::DexType))
                        .on_delete(ForeignKeyAction::NoAction)
                        .on_update(ForeignKeyAction::NoAction)
                )
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
        manager.drop_table(Table::drop().table(Hop::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(Dex::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(Token::Table).to_owned()).await?;

        Ok(())
    }
}
