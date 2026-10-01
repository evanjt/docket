//! The rules of a docket, free of any database or transport.

pub mod api;
pub mod clock;
pub mod fact;
pub mod flow;
pub mod item;
pub mod like;
pub mod member;
pub mod pace;
pub mod pyjson;
pub mod queue;
pub mod rows;
pub mod rules;
pub mod search;
pub mod text;
pub mod word;

/// The database every docket server opens, created on an empty file.
pub const SCHEMA: &str = include_str!("../schema.sql");
