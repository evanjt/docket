use crudcrate::EntityToModels;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, EntityToModels)]
#[sea_orm(table_name = "assignments")]
#[crudcrate(
    api_struct = "Assignment",
    description = "One attempt at an item: an agent's claim or an ask to the owner, and how it ended",
    generate_router
)]
pub struct Model {
    #[sea_orm(primary_key)]
    #[crudcrate(primary_key, sortable)]
    pub id: i64,
    #[crudcrate(filterable)]
    pub rid: i64,
    #[crudcrate(filterable)]
    pub assignee: String,
    #[crudcrate(filterable)]
    pub kind: String,
    #[crudcrate(sortable, filterable)]
    pub started_at: String,
    #[crudcrate(sortable)]
    pub ended_at: Option<String>,
    #[crudcrate(filterable)]
    pub outcome: Option<String>,
    pub note: Option<String>,
    #[crudcrate(filterable)]
    pub actor: Option<String>,
    pub branch: Option<String>,
    #[crudcrate(filterable)]
    pub host: String,
    #[crudcrate(filterable)]
    pub machine: Option<String>,
    #[crudcrate(filterable)]
    pub runner: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub role: Option<String>,
    pub job: Option<String>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub cost_reported: Option<f64>,
    pub job_started_at: Option<String>,
    pub job_ended_at: Option<String>,
    pub job_exit: Option<i32>,
    #[crudcrate(filterable)]
    pub need: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
