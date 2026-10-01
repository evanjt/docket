use crudcrate::EntityToModels;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, EntityToModels)]
#[sea_orm(table_name = "events")]
#[crudcrate(
    api_struct = "Event",
    description = "A move recorded on an item or a project",
    generate_router
)]
pub struct Model {
    #[sea_orm(primary_key)]
    #[crudcrate(primary_key, sortable, filterable)]
    pub seq: i64,
    pub uid: String,
    #[crudcrate(filterable)]
    pub project: String,
    #[crudcrate(filterable)]
    pub rid: Option<i64>,
    #[crudcrate(sortable, filterable)]
    pub at: String,
    #[crudcrate(filterable)]
    pub host: String,
    pub branch: Option<String>,
    #[crudcrate(filterable)]
    pub kind: String,
    pub note: Option<String>,
    pub data: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
