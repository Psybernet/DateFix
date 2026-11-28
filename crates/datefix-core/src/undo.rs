//! The single-batch undo file used before 2.2 (`last-rename.json`). New
//! history goes in [`crate::History`]; this is kept to read the old file.

use crate::files::Outcome;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rename {
    pub from: PathBuf,
    pub to: PathBuf,
}

/// A record of one batch of renames.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    /// The folder that was scanned.
    pub folder: PathBuf,
    /// When the renames happened, in seconds since 1970.
    pub when: u64,
    /// The renames, in the order they were made.
    pub renames: Vec<Rename>,
}

impl Journal {
    /// Builds a journal from the renames that succeeded, or `None` if none did.
    pub fn from_outcomes(folder: &Path, outcomes: &[Outcome]) -> Option<Journal> {
        let renames: Vec<Rename> = outcomes
            .iter()
            .filter_map(|o| match o {
                Outcome::Renamed { from, to } => Some(Rename {
                    from: from.clone(),
                    to: to.clone(),
                }),
                _ => None,
            })
            .collect();
        if renames.is_empty() {
            return None;
        }
        let when = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Some(Journal {
            folder: folder.to_path_buf(),
            when,
            renames,
        })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, json)
    }

    pub fn load(path: &Path) -> Option<Journal> {
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// Puts every name back, newest rename first, so folders get their old
    /// names back before the items inside them do.
    pub fn undo(&self) -> Vec<Outcome> {
        self.renames
            .iter()
            .rev()
            .map(|r| {
                if !r.to.exists() {
                    Outcome::Failed {
                        from: r.to.clone(),
                        error: "it has been moved or deleted since".into(),
                    }
                } else if r.from.exists() {
                    Outcome::Skipped {
                        from: r.to.clone(),
                        reason: "something else now has the old name".into(),
                    }
                } else {
                    match fs::rename(&r.to, &r.from) {
                        Ok(()) => Outcome::Renamed {
                            from: r.to.clone(),
                            to: r.from.clone(),
                        },
                        Err(e) => Outcome::Failed {
                            from: r.to.clone(),
                            error: e.to_string(),
                        },
                    }
                }
            })
            .collect()
    }
}
