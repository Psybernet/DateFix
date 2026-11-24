//! DateFix core: finds dates in file and folder names and rewrites them as
//! YYYY-MM-DD. Shared by the desktop app and the command-line tool.

mod files;
mod parse;
mod undo;

pub use files::{apply, next_free_name, plan, rename_one, Change, OnClash, Outcome};
pub use parse::{extract_date, new_name_for, FoundDate};
pub use undo::{Journal, Rename};
