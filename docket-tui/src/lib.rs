//! The docket screen: every project, its flow and queue, the items in full, the owner's queue, the plans
//! and the settings, read from the server and read again whenever the database moves.

pub mod app;
pub mod doc;
pub mod filter;
pub mod load;
pub mod mouse;
pub mod page;
pub mod palette;
pub mod run;
pub mod source;
pub mod style;
pub mod view;
pub mod write;

#[cfg(test)]
#[path = "tests/fixture.rs"]
mod tests_fixture;

#[cfg(test)]
#[path = "tests/board.rs"]
mod tests_board;
