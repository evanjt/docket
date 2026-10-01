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
    #[sea_orm(primary_key)]
    #[crudcrate(primary_key, sortable)]
    pub id: i64,
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
