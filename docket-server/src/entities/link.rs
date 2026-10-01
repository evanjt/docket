use crudcrate::EntityToModels;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, EntityToModels)]
#[sea_orm(table_name = "links")]
#[crudcrate(
    api_struct = "Link",
    description = "A tie from an item to another item or to a path",
    generate_router
)]
pub struct Model {
    // The table declares no key, so SQLite's own row id identifies a link.
    #[sea_orm(primary_key, column_name = "rowid")]
    #[crudcrate(primary_key, sortable)]
    pub rowid: i64,
    #[crudcrate(filterable)]
    pub rid: i64,
    #[crudcrate(filterable)]
    pub kind: String,
    #[crudcrate(filterable)]
    pub to_rid: Option<i64>,
    #[crudcrate(filterable)]
    pub to_path: Option<String>,
    pub to_line: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
