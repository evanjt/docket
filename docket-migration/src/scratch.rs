//! A database of its own for one test, on the Postgres that `DATABASE_URL` names, dropped with its guard.

use std::sync::atomic::{AtomicU32, Ordering};

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A database created for one test, and the pool over it. Dropping it drops the database.
pub struct Scratch {
    pub db: DatabaseConnection,
    admin: String,
    name: String,
}

impl Scratch {
    /// A new database with the schema applied, reached through a pool of `connections`.
    ///
    /// # Panics
    /// `DATABASE_URL` is unset, or the server refuses to create the database.
    pub async fn new(connections: u32) -> Self {
        let s = Self::bare(connections).await;
        crate::migrate(&s.db)
            .await
            .expect("migrate the scratch database");
        s
    }

    /// A new database with nothing in it, not even the migrations table.
    ///
    /// # Panics
    /// `DATABASE_URL` is unset, or the server refuses to create the database.
    pub async fn bare(connections: u32) -> Self {
        let admin = std::env::var("DATABASE_URL").expect(
            "the tests need DATABASE_URL: a Postgres on which this user can create databases",
        );
        let name = format!(
            "docket_test_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let root = Database::connect(&admin).await.expect("reach DATABASE_URL");
        root.execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await
            .expect("create the scratch database");
        root.close().await.ok();
        let mut options = ConnectOptions::new(with_database(&admin, &name));
        options.max_connections(connections).sqlx_logging(false);
        let db = Database::connect(options)
            .await
            .expect("reach the scratch database");
        Self { db, admin, name }
    }

    /// Rows written by hand with their own keys, each identity then moved past them.
    ///
    /// # Panics
    /// The database refuses the SQL.
    pub async fn seed(&self, sql: &str) {
        self.db
            .execute_unprepared(sql)
            .await
            .expect("seed the scratch database");
        crate::reset_identities(&self.db)
            .await
            .expect("move the identities past the seeded rows");
    }

    /// The URL of this database.
    #[must_use]
    pub fn url(&self) -> String {
        with_database(&self.admin, &self.name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let admin = self.admin.clone();
        let drop = format!("DROP DATABASE IF EXISTS {} WITH (FORCE)", self.name);
        let gone = std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async {
                if let Ok(root) = Database::connect(&admin).await {
                    root.execute_unprepared(&drop).await.ok();
                    root.close().await.ok();
                }
            });
        });
        gone.join().ok();
    }
}

/// The URL with its database name replaced, its query kept.
#[must_use]
pub fn with_database(url: &str, name: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((b, q)) => (b, format!("?{q}")),
        None => (url, String::new()),
    };
    let start = base.find("://").map_or(0, |i| i + 3);
    let server = match base[start..].find('/') {
        Some(i) => &base[..start + i],
        None => base,
    };
    format!("{server}/{name}{query}")
}

#[cfg(test)]
#[path = "tests/scratch.rs"]
mod tests;
