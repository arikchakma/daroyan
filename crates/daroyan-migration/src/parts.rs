use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{text, timestamp};

pub fn make_id() -> ColumnDef {
    text("id").primary_key().take()
}

pub fn make_created_at() -> ColumnDef {
    timestamp("created_at").default(Expr::current_timestamp()).take()
}

pub fn make_updated_at() -> ColumnDef {
    timestamp("updated_at").default(Expr::current_timestamp()).take()
}

pub fn make_index(
    name: &'static str,
    table: &'static str,
    columns: &[&'static str],
) -> IndexCreateStatement {
    let mut index = Index::create();
    index.name(name).table(table);
    for column in columns {
        index.col(*column);
    }
    index
}

pub fn make_unique(
    name: &'static str,
    table: &'static str,
    columns: &[&'static str],
) -> IndexCreateStatement {
    make_index(name, table, columns).unique().to_owned()
}

pub async fn create_tables(
    manager: &SchemaManager<'_>,
    tables: Vec<TableCreateStatement>,
) -> Result<(), DbErr> {
    for table in tables {
        manager.create_table(table).await?;
    }
    Ok(())
}

pub async fn create_indexes(
    manager: &SchemaManager<'_>,
    indexes: Vec<IndexCreateStatement>,
) -> Result<(), DbErr> {
    for index in indexes {
        manager.create_index(index).await?;
    }
    Ok(())
}

pub async fn drop_tables(
    manager: &SchemaManager<'_>,
    tables: &[&'static str],
) -> Result<(), DbErr> {
    for table in tables {
        manager.drop_table(Table::drop().table(*table).to_owned()).await?;
    }
    Ok(())
}
