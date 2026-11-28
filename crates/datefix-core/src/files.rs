//! Planning and applying renames on disk.

use crate::parse::new_name_for;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One planned rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Something already has the new name.
    #[serde(default)]
    pub clash: bool,
}

/// What to do when the new name is already taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnClash {
    /// Leave the item as it is.
    Skip,
    /// Add _2, _3… before the extension.
    #[default]
    Number,
}

/// What happened to one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "lowercase")]
pub enum Outcome {
    Renamed { from: PathBuf, to: PathBuf },
    Skipped { from: PathBuf, reason: String },
    Failed { from: PathBuf, error: String },
}

impl Outcome {
    pub fn is_renamed(&self) -> bool {
        matches!(self, Outcome::Renamed { .. })
    }
}

/// Lists every file and folder in `dir` whose name needs its date fixed.
///
/// With `recursive`, subfolders are included and the deepest items come
/// first, so renaming a folder never breaks the paths of what's inside it.
pub fn plan(dir: &Path, recursive: bool, month_first: bool) -> io::Result<Vec<Change>> {
    let mut changes = Vec::new();
    plan_into(dir, recursive, month_first, &mut changes)?;
    Ok(changes)
}

fn plan_into(
    dir: &Path,
    recursive: bool,
    month_first: bool,
    out: &mut Vec<Change>,
) -> io::Result<()> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            dirs.push(entry.path());
        } else {
            files.push(entry.path());
        }
    }
    files.sort();
    dirs.sort();

    if recursive {
        for sub in &dirs {
            // A folder we can't read shouldn't stop the whole scan.
            let _ = plan_into(sub, recursive, month_first, out);
        }
    }
    for path in files.iter().chain(dirs.iter()) {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(new_name) = new_name_for(name, month_first) {
            let to = path.with_file_name(new_name);
            out.push(Change {
                clash: to.exists(),
                from: path.clone(),
                to,
            });
        }
    }
    Ok(())
}

/// `path` with `_n` added before the extension: `a.txt` becomes `a_2.txt`.
pub(crate) fn numbered(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    path.with_file_name(format!("{stem}_{n}{ext}"))
}

/// The first free name made by adding _2, _3… before the extension.
pub fn next_free_name(path: &Path) -> PathBuf {
    let mut candidate = path.to_path_buf();
    let mut version = 2;
    while candidate.exists() {
        candidate = numbered(path, version);
        version += 1;
    }
    candidate
}

/// Renames one item, never overwriting anything that already exists.
pub fn rename_one(from: &Path, to: &Path, on_clash: OnClash) -> Outcome {
    if !from.exists() {
        return Outcome::Failed {
            from: from.into(),
            error: "it no longer exists".into(),
        };
    }
    let target = if to.exists() {
        match on_clash {
            OnClash::Skip => {
                return Outcome::Skipped {
                    from: from.into(),
                    reason: "the new name is already taken".into(),
                }
            }
            OnClash::Number => next_free_name(to),
        }
    } else {
        to.to_path_buf()
    };
    match fs::rename(from, &target) {
        Ok(()) => Outcome::Renamed {
            from: from.into(),
            to: target,
        },
        Err(e) => Outcome::Failed {
            from: from.into(),
            error: e.to_string(),
        },
    }
}

/// Applies a list of changes in order.
pub fn apply(changes: &[Change], on_clash: OnClash) -> Vec<Outcome> {
    changes
        .iter()
        .map(|c| rename_one(&c.from, &c.to, on_clash))
        .collect()
}
