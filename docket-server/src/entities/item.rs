use crudcrate::EntityToModels;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, EntityToModels)]
#[sea_orm(table_name = "items")]
#[crudcrate(
    api_struct = "Item",
    description = "An item as stored",
    generate_router
)]
pub struct Model {
    #[sea_orm(primary_key)]
    #[crudcrate(primary_key, sortable, filterable)]
    pub rid: i64,
    #[crudcrate(filterable)]
    pub project: String,
    #[crudcrate(sortable, filterable)]
    pub key: String,
    #[crudcrate(sortable, filterable)]
    pub num: i64,
    #[crudcrate(filterable)]
    pub id: String,
    #[crudcrate(fulltext)]
    pub title: String,
    #[crudcrate(filterable)]
    pub state: String,
    #[crudcrate(filterable)]
    pub turn: Option<String>,
    pub turn_note: Option<String>,
    pub asked_at: Option<String>,
    #[crudcrate(filterable)]
    pub claim_branch: Option<String>,
    #[crudcrate(filterable)]
    pub claim_host: Option<String>,
    pub claim_since: Option<String>,
    pub claim_runner: Option<String>,
    pub claim_job: Option<String>,
    pub claim_on: Option<String>,
    #[crudcrate(filterable)]
    pub wait_on: Option<String>,
    #[crudcrate(filterable)]
    pub wait_item: Option<i64>,
    pub wait_ref: Option<String>,
    pub wait_since: Option<String>,
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    pub resolution: Option<String>,
    pub superseded_by: Option<i64>,
    #[crudcrate(filterable)]
    pub parent_rid: Option<i64>,
    #[crudcrate(filterable)]
    pub scope: Option<String>,
    #[crudcrate(filterable)]
    pub complexity: Option<String>,
    #[crudcrate(filterable)]
    pub group_name: Option<String>,
    #[crudcrate(filterable)]
    pub theme: Option<String>,
    #[crudcrate(filterable)]
    pub release_id: Option<i64>,
    #[crudcrate(filterable)]
    pub area_id: Option<i64>,
    #[crudcrate(sortable)]
    pub rank: Option<i64>,
    #[sea_orm(column_name = "type")]
    #[crudcrate(filterable)]
    pub item_type: String,
    #[crudcrate(filterable)]
    pub priority: String,
    pub tags: serde_json::Value,
    #[crudcrate(exclude(list))]
    pub body: String,
    pub conflict: i64,
    #[crudcrate(sortable)]
    pub opened_at: String,
    #[crudcrate(sortable)]
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
