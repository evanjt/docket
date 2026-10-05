//! What every command reads: the server, the flags, the checkout it runs in, and its project.

use std::cell::OnceCell;
use std::path::PathBuf;

use serde_json::Value;

use docket_client::Config;
use docket_client::roots::{Root, Roots};
use docket_core::api::{Common, ProjectRequest, ProjectResolved};
use docket_core::clock;
use docket_core::project::slug_from_url;
use docket_core::text::split_id;

use crate::fail::{Fail, Result};
use crate::http::Api;
use crate::local;
use crate::py::{Py, dumps_indent};

pub struct Ctx {
    pub api: Api,
    pub json: bool,
    pub cwd: PathBuf,
    pub roots: Roots,
    project_flag: Option<String>,
    branch_flag: Option<String>,
    repo: Option<String>,
    project: OnceCell<String>,
    host: OnceCell<String>,
}

impl Ctx {
    /// The server from the client file and the environment, and the roots bound on this machine.
    ///
    /// # Errors
    /// No server or key is configured.
    pub fn new(json: bool, project: Option<String>, branch: Option<String>) -> Result<Self> {
        let config = Config::load().map_err(|e| Fail::refused(format!("docket: {e}")))?;
        let file = docket_client::config::path().and_then(|p| std::fs::read_to_string(p).ok());
        let repo = std::env::var("DOCKET_REPO")
            .ok()
            .filter(|r| !r.is_empty())
            .or_else(|| {
                file.as_deref()
                    .and_then(|t| docket_client::config::field(t, "repo"))
            });
        let roots = Roots::load(docket_client::roots::path().unwrap_or_default());
        Ok(Self {
            api: Api::new(&config)?,
            json,
            cwd: std::env::current_dir().unwrap_or_default(),
            roots,
            project_flag: project,
            branch_flag: branch,
            repo,
            project: OnceCell::new(),
            host: OnceCell::new(),
        })
    }

    /// The branch acting: `--branch`, else the checkout's HEAD, else main.
    #[must_use]
    pub fn branch(&self) -> String {
        self.branch_flag
            .clone()
            .or_else(|| local::branch(&self.cwd))
            .unwrap_or_else(|| "main".to_string())
    }

    /// What every write carries.
    ///
    /// # Errors
    /// The project cannot be resolved.
    pub fn common(&mut self, force: bool) -> Result<Common> {
        Ok(Common {
            project: self.project()?,
            branch: Some(self.branch()),
            force,
        })
    }

    /// The machine the key belongs to, as the server records it.
    ///
    /// # Errors
    /// The server cannot be reached.
    pub fn host(&self) -> Result<String> {
        if let Some(h) = self.host.get() {
            return Ok(h.clone());
        }
        let who = self.api.get("/whoami", &[])?;
        let host = who["host"].as_str().unwrap_or_default().to_string();
        let _ = self.host.set(host.clone());
        Ok(host)
    }

    /// `[("project", slug)]` and the pairs given, for a read route.
    ///
    /// # Errors
    /// The project cannot be resolved.
    pub fn query(
        &mut self,
        pairs: &[(&'static str, Option<String>)],
    ) -> Result<Vec<(&'static str, String)>> {
        let mut out = vec![("project", self.project()?)];
        out.extend(pairs.iter().filter_map(|(k, v)| v.clone().map(|v| (*k, v))));
        Ok(out)
    }

    /// A read route of the project.
    ///
    /// # Errors
    /// The project cannot be resolved, or the server refuses.
    pub fn read(&mut self, path: &str, pairs: &[(&'static str, Option<String>)]) -> Result<Value> {
        let q = self.query(pairs)?;
        self.api.get(path, &q)
    }

    /// The project's stored row.
    ///
    /// # Errors
    /// The project is unknown.
    pub fn project_row(&self, slug: &str) -> Result<Value> {
        let filter = serde_json::json!({ "slug": slug }).to_string();
        let found = self.api.get("/projects", &[("filter", filter)])?;
        found
            .as_array()
            .and_then(|a| a.iter().find(|p| p["slug"] == slug))
            .cloned()
            .ok_or_else(|| Fail::refused(format!("no project {slug}")))
    }

    /// `--json` output, as `json.dumps(indent=2)` prints it.
    #[allow(clippy::unused_self)]
    pub fn emit(&self, v: &Py) {
        println!("{}", dumps_indent(v));
    }

    /// The project this command acts on, resolved once.
    ///
    /// # Errors
    /// No project is named, bound or found for the checkout.
    pub fn project(&mut self) -> Result<String> {
        if let Some(p) = self.project.get() {
            return Ok(p.clone());
        }
        let slug = self.resolve()?;
        let _ = self.project.set(slug.clone());
        Ok(slug)
    }

    /// `-p SLUG`, `DOCKET_PROJECT`, the longest root bound here, else the outermost git repository,
    /// matched to a project by slug, remote or directory name, or created, and bound.
    fn resolve(&mut self) -> Result<String> {
        let named = self
            .project_flag
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("DOCKET_PROJECT")
                    .ok()
                    .filter(|s| !s.is_empty())
            });
        if let Some(slug) = named {
            self.project_row(&slug)?;
            return Ok(slug);
        }
        let (top, urls) = local::outermost_repo(&self.cwd);
        let top_s = top.as_ref().map(|t| t.display().to_string());
        if let Some(slug) = bound_at(
            &self.roots.roots,
            &local::realpath(&self.cwd),
            top_s.as_deref(),
        ) {
            return Ok(slug);
        }
        let shown = self.cwd.display().to_string();
        if let Some(repo) = &self.repo {
            let repo = local::realpath(&local::expand("", repo));
            let real = local::realpath(&self.cwd);
            if real == repo || real.starts_with(&format!("{repo}/")) {
                return Err(Fail::refused(format!(
                    "{shown} is inside the docket repository itself, which holds every project and is \
                     none of them. Pass -p SLUG, or run from the project checkout."
                )));
            }
        }
        let Some(top) = top else {
            return Err(Fail::refused(format!(
                "{shown} is not inside a git repository and no project is bound here. \
                 Run it from a checkout, or pass -p SLUG, or bind one with docket bind."
            )));
        };
        self.bind_checkout(&top, urls)
    }

    fn bind_checkout(&mut self, top: &std::path::Path, urls: Vec<String>) -> Result<String> {
        let top_s = top.display().to_string();
        let req = ProjectRequest {
            candidate: urls.iter().find_map(|u| slug_from_url(u)),
            remotes: urls,
            basename: top
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            create: true,
        };
        let got: ProjectResolved = self.api.post("project", &req)?;
        let Some(slug) = got.slug else {
            return Err(Fail::refused(format!(
                "{top_s} could be any of {}. Say which: docket bind SLUG",
                got.matches.join(", ")
            )));
        };
        self.bind_root(&top_s, &slug, "auto")?;
        eprintln!(
            "bound {top_s} to {slug} ({}, outermost git repo on {})",
            got.how,
            self.host()?
        );
        Ok(slug)
    }
}

/// The project bound to a directory, else to the outermost checkout it belongs to, so a worktree kept
/// outside its checkout's root takes the checkout's binding instead of binding the checkout afresh.
#[must_use]
pub fn bound_at(roots: &[Root], cwd: &str, top: Option<&str>) -> Option<String> {
    docket_client::roots::bound(roots, cwd)
        .or_else(|| top.and_then(|t| docket_client::roots::bound(roots, t)))
}

/// The root of a project that holds the directory, else the outermost checkout it belongs to: the
/// checkout a command runs in, not whichever root of the project is shortest.
#[must_use]
pub fn root_at(roots: &[Root], project: &str, cwd: &str, top: Option<&str>) -> Option<String> {
    let mine: Vec<Root> = roots
        .iter()
        .filter(|r| r.project == project)
        .cloned()
        .collect();
    let held = |dir: &str| {
        mine.iter()
            .map(|r| r.path.trim_end_matches('/'))
            .filter(|p| dir == *p || dir.starts_with(&format!("{p}/")))
            .max_by_key(|p| p.len())
            .map(str::to_string)
    };
    held(cwd).or_else(|| top.and_then(held))
}

impl Ctx {
    /// A directory bound to a project in this machine's roots file.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn bind_root(&mut self, path: &str, slug: &str, how: &str) -> Result<()> {
        self.roots
            .bind(Root {
                path: path.to_string(),
                project: slug.to_string(),
                how: how.to_string(),
                bound_at: clock::now(),
            })
            .map_err(|e| {
                Fail::refused(format!(
                    "docket: cannot write {}: {e}",
                    self.roots.file().display()
                ))
            })
    }
}

/// An id as the server stores it, `b07` read as `B7`.
///
/// # Errors
/// The text is not a key of one to three capitals and a number.
pub fn id(text: &str) -> Result<String> {
    let (key, num) = split_id(text)?;
    Ok(format!("{key}{num}"))
}

#[cfg(test)]
#[path = "tests/ctx.rs"]
mod tests;
