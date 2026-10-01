//! The docket screen: every project, its flow and queue, the items in full, the owner's queue, the plans
//! and the settings, read from the server and read again whenever the database moves.

pub mod app;
pub mod board;
pub mod doc;
pub mod load;
pub mod page;
pub mod source;
pub mod style;
pub mod view;

#[cfg(test)]
#[path = "tests/fixture.rs"]
mod tests_fixture;
