//! The server's `GET /dump`, and the one seam a pass reads it through.

use docket_client::Api;
use docket_core::dump::DumpPage;

/// Where a pass gets the rows changed after a cursor.
pub trait Source {
    /// The page after `since`; every row when `since` is 0.
    ///
    /// # Errors
    /// The rows could not be read.
    fn page(&self, since: i64) -> Result<DumpPage, String>;
}

impl Source for Api {
    fn page(&self, since: i64) -> Result<DumpPage, String> {
        self.get("/dump", &[("since", since.to_string())])
            .map_err(|e| format!("GET /dump: {e}"))
    }
}
