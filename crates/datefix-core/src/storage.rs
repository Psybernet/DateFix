//! Where DateFix keeps its files: a "DateFix data" folder next to the program
//! itself, so everything stays in one place and goes when the program does.

use std::fs;
use std::path::{Path, PathBuf};

/// The name of the folder created next to the program.
pub const DATA_FOLDER: &str = "DateFix data";

/// The file holding the last batch of renames, for undo.
pub const UNDO_FILE: &str = "last-rename.json";

/// Creates `dir` if needed and checks that files can be written in it.
pub fn usable_dir(dir: &Path) -> bool {
    let probe = dir.join(".write-test");
    let ok = fs::create_dir_all(dir).is_ok() && fs::write(&probe, b"").is_ok();
    let _ = fs::remove_file(&probe);
    ok
}

/// The "DateFix data" folder next to the running program, if it can be written to.
pub fn data_dir_next_to_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.join(DATA_FOLDER);
    usable_dir(&dir).then_some(dir)
}
