use crudcrate::EntityToModels;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, EntityToModels)]
#[sea_orm(table_name = "projects")]
#[crudcrate(api_struct = "Project", description = "A project", generate_router)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[crudcrate(primary_key, sortable, filterable)]
    pub slug: String,
    pub remotes: serde_json::Value,
    pub cite_roots: serde_json::Value,
    pub repos: serde_json::Value,
    pub fleet_repo: Option<String>,
    pub integration_ref: Option<String>,
    pub worktree_hint: Option<String>,
    pub test_hint: Option<String>,
    pub skills: serde_json::Value,
    pub created_at: String,
    #[crudcrate(sortable)]
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
