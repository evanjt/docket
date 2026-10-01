//! The dump files in a checkout: read, written whole by rename, and listed for a restore.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process;

/// A file's text, or none when it does not exist or is not text.
#[must_use]
pub fn read(repo: &Path, rel: &str) -> Option<String> {
    fs::read_to_string(repo.join(rel)).ok()
}

/// The text written by a temporary file renamed over the path; false when it already held it.
///
/// # Errors
/// The path is a symlink, or the file system refuses the write.
pub fn write(repo: &Path, rel: &str, text: &str) -> Result<bool, String> {
    let path = repo.join(rel);
    if path.is_symlink() {
        return Err(format!(
            "{} is a symlink; the dump never writes through one",
            path.display()
        ));
    }
    if read(repo, rel).as_deref() == Some(text) {
        return Ok(false);
    }
    let dir = path.parent().ok_or(format!("{rel} has no directory"))?;
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".docket-{}.tmp", process::id()));
    let written = fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &path));
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    Ok(true)
}

/// Every directory holding a `project.json`, as a slug, skipping hidden, `src` and `template` trees.
///
/// # Errors
/// A directory cannot be read.
pub fn projects(repo: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    walk(repo, repo, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(repo: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    if dir != repo && dir.join("project.json").is_file() {
        let rel = dir.strip_prefix(repo).map_err(|e| e.to_string())?;
        out.push(rel.to_string_lossy().replace('\\', "/"));
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        let skip = name.starts_with('.') || name == "src" || name == "template";
        if !skip && entry.file_type().is_ok_and(|t| t.is_dir()) {
            walk(repo, &entry.path(), out)?;
        }
    }
    Ok(())
}

/// The ids of a project's item files, sorted as text.
///
/// # Errors
/// The items directory exists but cannot be read.
pub fn items(repo: &Path, slug: &str) -> Result<Vec<String>, String> {
    let dir = repo.join(slug).join("items");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".md")
                .map(String::from)
        })
        .collect();
    ids.sort();
    Ok(ids)
}

#[cfg(test)]
#[path = "tests/tree.rs"]
mod tests;
