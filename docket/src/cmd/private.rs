//! `docket private`: the owner's private names, read from the server on the owner's key, searched for
//! in a checkout so a public repository never carries one. The names are never written to a file in
//! the repository; names a public repository may carry are listed outside it, in
//! `$XDG_CONFIG_HOME/docket/public`, and private names the docket cannot know in `.../private`. A licence, which names its holder by design, is not searched.

use std::path::{Path, PathBuf};

use docket_core::private::{Private, Titles, hits, ids, is_comment, shapes, ssh_hosts, terms};

use crate::args::PrivateCmd;
use crate::ctx::Ctx;
use crate::dispatch::git;
use crate::fail::{Fail, Result};

/// The extensions whose comments are searched for item ids.
const CODE: [&str; 11] = [
    "rs", "ts", "js", "svelte", "toml", "yml", "yaml", "sh", "sql", "css", "html",
];

/// A file larger than this is not read.
const LARGEST: u64 = 2 * 1024 * 1024;

/// # Errors
/// The server cannot be reached or refuses the key, or git fails.
pub fn private(ctx: &mut Ctx, what: &PrivateCmd) -> Result<i32> {
    match what {
        PrivateCmd::Check {
            staged,
            message,
            range,
            ids,
            paths,
        } => check(
            ctx,
            &Scope {
                staged: *staged,
                message: message.as_deref(),
                range: range.as_deref(),
                ids: *ids,
                paths,
            },
        ),
        PrivateCmd::Terms => {
            let (terms, _) = read_terms(ctx)?;
            for t in terms {
                println!("{t}");
            }
            Ok(0)
        }
        PrivateCmd::Hook { force } => hook(*force),
    }
}

/// What a check looks for: the private names, the item keys and the item titles.
struct Look {
    terms: Vec<String>,
    keys: Vec<String>,
    titles: Titles,
}

/// The terms to look for and the item keys, from the server and this machine.
fn read_terms(ctx: &mut Ctx) -> Result<(Vec<String>, Vec<String>)> {
    read(ctx).map(|l| (l.terms, l.keys))
}

/// The private names, keys and titles, from the server and this machine: its user name, its home,
/// and the host aliases its ssh config names.
fn read(ctx: &mut Ctx) -> Result<Look> {
    let v = ctx.api.get("/private", &[])?;
    let private: Private =
        serde_json::from_value(v).map_err(|e| Fail::refused(format!("/private: {e}")))?;
    let mut local = Vec::new();
    if let Ok(user) = std::env::var("USER") {
        local.push(user);
    }
    if let Some(home) = std::env::var_os("HOME") {
        local.push(home.to_string_lossy().into_owned());
        let config = PathBuf::from(home).join(".ssh").join("config");
        local.extend(ssh_hosts(
            &std::fs::read_to_string(config).unwrap_or_default(),
        ));
    }
    local.extend(listed("private"));
    let mut allowed = listed("public");
    allowed.extend(origin_names());
    Ok(Look {
        terms: terms(&private, &local, &allowed),
        keys: private.keys,
        titles: Titles::new(&private.titles),
    })
}

/// The names listed one a line in `$XDG_CONFIG_HOME/docket/NAME`, `#` starting a comment: `public`
/// for names a public repository may carry, `private` for private names the docket cannot know, as
/// another machine's alias for this one.
fn listed(name: &str) -> Vec<String> {
    let Some(path) = config_path(name) else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn config_path(name: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("docket").join(name))
}

/// The owner and name of the repository's `origin`, which its address already makes public.
fn origin_names() -> Vec<String> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let Ok(url) = git(&cwd, &["remote", "get-url", "origin"]) else {
        return Vec::new();
    };
    let path = url
        .trim_end_matches(".git")
        .rsplit([':', '/'])
        .take(2)
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut out = path.clone();
    if path.len() == 2 {
        out.push(format!("{}/{}", path[1], path[0]));
    }
    out
}

/// One hit: where it is and the term or id found.
struct Hit {
    at: String,
    found: String,
}

/// What `docket private check` searches.
struct Scope<'a> {
    staged: bool,
    message: Option<&'a str>,
    range: Option<&'a str>,
    ids: bool,
    paths: &'a [String],
}

#[allow(clippy::too_many_lines)]
fn check(ctx: &mut Ctx, scope: &Scope) -> Result<i32> {
    let look = read(ctx)?;
    let root = root()?;
    let mut found = Vec::new();
    let mut read = 0;
    let want_ids = scope.ids;
    let scan = |name: &str, n: usize, line: &str, code: bool, found: &mut Vec<Hit>| {
        let at = format!("{name}:{n}");
        for t in hits(&look.terms, line) {
            found.push(Hit {
                at: at.clone(),
                found: t.to_string(),
            });
        }
        for t in look.titles.hits(line) {
            found.push(Hit {
                at: at.clone(),
                found: format!("\"{t}\" (a docket item's title)"),
            });
        }
        for t in shapes(line) {
            found.push(Hit {
                at: at.clone(),
                found: format!("{t} (a private address or a token)"),
            });
        }
        if want_ids && code && is_comment(line) {
            for id in ids(&look.keys, line) {
                found.push(Hit {
                    at: at.clone(),
                    found: format!("{id} (a docket item cited in a comment)"),
                });
            }
        }
    };
    if let Some(range) = scope.range {
        let mut args = vec![
            "log",
            "-p",
            "--no-color",
            "--no-ext-diff",
            "--format=%x00commit %h%n%B",
        ];
        args.extend(range.split_whitespace());
        let log = git(&root, &args).map_err(Fail::refused)?;
        let (mut commit, mut file, mut n, mut in_message) =
            (String::new(), String::new(), 0usize, false);
        for line in log.lines() {
            if let Some(h) = line.strip_prefix("\0commit ") {
                commit = h.to_string();
                in_message = true;
                n = 0;
                read += 1;
            } else if line.starts_with("diff --git ") {
                in_message = false;
            } else if in_message {
                n += 1;
                scan(&format!("{commit} message"), n, line, true, &mut found);
            } else if let Some(f) = line.strip_prefix("+++ ") {
                file = f.strip_prefix("b/").unwrap_or(f).to_string();
            } else if let Some(h) = line.strip_prefix("@@ ") {
                n = h
                    .split_whitespace()
                    .find_map(|p| p.strip_prefix('+'))
                    .and_then(|p| p.split(',').next())
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(1);
            } else if let Some(added) = line.strip_prefix('+') {
                if !is_licence(&file) {
                    scan(
                        &format!("{commit} {file}"),
                        n,
                        added,
                        is_code(&file),
                        &mut found,
                    );
                }
                n += 1;
            } else if line.starts_with(' ') {
                n += 1;
            }
        }
    } else if let Some(file) = scope.message {
        let text =
            std::fs::read_to_string(file).map_err(|e| Fail::refused(format!("{file}: {e}")))?;
        for (i, line) in text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.starts_with('#'))
        {
            scan("commit message", i + 1, line, true, &mut found);
        }
        read = 1;
    } else if scope.staged {
        let diff = git(
            &root,
            &["diff", "--cached", "-U0", "--no-color", "--no-ext-diff"],
        )
        .map_err(Fail::refused)?;
        let (mut file, mut n) = (String::new(), 0usize);
        for line in diff.lines() {
            if let Some(f) = line.strip_prefix("+++ ") {
                file = f.strip_prefix("b/").unwrap_or(f).to_string();
                read += 1;
            } else if let Some(h) = line.strip_prefix("@@ ") {
                n = h
                    .split_whitespace()
                    .find_map(|p| p.strip_prefix('+'))
                    .and_then(|p| p.split(',').next())
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(1);
            } else if let Some(added) = line.strip_prefix('+') {
                scan(&file, n, added, is_code(&file), &mut found);
                n += 1;
            }
        }
    } else {
        let mut args = vec!["ls-files", "-z", "--"];
        args.extend(scope.paths.iter().map(String::as_str));
        let listed = git(&root, &args).map_err(Fail::refused)?;
        for name in listed
            .split('\0')
            .filter(|n| !n.is_empty() && !is_licence(n))
        {
            let Some(text) = text_of(&root.join(name)) else {
                continue;
            };
            read += 1;
            for (i, line) in text.lines().enumerate() {
                scan(name, i + 1, line, is_code(name), &mut found);
            }
        }
    }
    if found.is_empty() {
        let what = if scope.range.is_some() {
            "commit"
        } else {
            "file"
        };
        println!(
            "no private name in {read} {what}{}",
            if read == 1 { "" } else { "s" }
        );
        return Ok(0);
    }
    for h in &found {
        println!("{}: {}", h.at, h.found);
    }
    eprintln!(
        "docket private: {} private name{} found. Take each out; a name that is public goes in {}",
        found.len(),
        if found.len() == 1 { "" } else { "s" },
        config_path("public").map_or_else(|| "docket/public".into(), |p| p.display().to_string())
    );
    Ok(1)
}

fn root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().map_err(|e| Fail::refused(e.to_string()))?;
    git(&cwd, &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .map_err(|_| Fail::refused("docket private runs inside a git checkout"))
}

/// A licence names its holder by design, so it is not searched.
fn is_licence(name: &str) -> bool {
    Path::new(name)
        .file_name()
        .and_then(|f| f.to_str())
        .is_some_and(|f| f.starts_with("LICENSE") || f.starts_with("COPYING"))
}

fn is_code(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CODE.contains(&e))
}

/// A file's text, unless it is missing, too large or binary.
fn text_of(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > LARGEST {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The pre-push hook: every commit a push adds, its message and its changes, then the tree.
const PRE_PUSH: &str = "z=0000000000000000000000000000000000000000
while read -r local_ref local remote_ref remote; do
  [ \"$local\" = \"$z\" ] && continue
  if [ \"$remote\" = \"$z\" ]; then range=\"$local --not --remotes\"; else range=\"$remote..$local\"; fi
  docket private check --range \"$range\" || exit 1
done
exec docket private check
";

/// The marker on every hook docket writes, so it knows its own.
const MARK: &str = "# written by docket private hook";

fn hook(force: bool) -> Result<i32> {
    let root = root()?;
    let dir = PathBuf::from(
        git(
            &root,
            &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
        )
        .map_err(Fail::refused)?,
    );
    std::fs::create_dir_all(&dir).map_err(|e| Fail::refused(e.to_string()))?;
    for (name, line) in [
        ("pre-commit", "docket private check --staged"),
        ("commit-msg", "docket private check --message \"$1\""),
        ("pre-push", PRE_PUSH),
    ] {
        let path = dir.join(name);
        let mine = std::fs::read_to_string(&path).map_or(true, |t| t.contains(MARK));
        if !mine && !force {
            return Err(Fail::refused(format!(
                "{} is not docket's: pass --force to replace it",
                path.display()
            )));
        }
        let text = if line.contains('\n') {
            format!("#!/bin/sh\n{MARK}\n{line}")
        } else {
            format!("#!/bin/sh\n{MARK}\nexec {line}\n")
        };
        std::fs::write(&path, text).map_err(|e| Fail::refused(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| Fail::refused(e.to_string()))?;
        }
        println!("{}", path.display());
    }
    Ok(0)
}
