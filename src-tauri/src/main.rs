// No console window behind the app in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use datefix_core::{
    apply, data_dir_next_to_exe, extract_date, plan, usable_dir, Change, Journal, OnClash, Outcome,
    DATA_FOLDER, UNDO_FILE,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State, WebviewWindowBuilder};

/// Which see-through effect the window got: "mica", "acrylic" or "none".
struct Glass(&'static str);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    from: PathBuf,
    to: PathBuf,
    old_name: String,
    new_name: String,
    /// Subfolder the item is in, relative to the scanned folder ("" for the top).
    location: String,
    is_dir: bool,
    clash: bool,
    /// The date text in the old name, and what it becomes.
    old_date: String,
    new_date: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Scan {
    folder: PathBuf,
    items: Vec<Item>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    renamed: usize,
    skipped: usize,
    failed: usize,
    /// One line for each item that was skipped or failed.
    problems: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UndoInfo {
    count: usize,
    folder: PathBuf,
    when: u64,
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Where DateFix keeps its undo history and the window's browser data.
struct DataDir(PathBuf);

/// "DateFix data" next to DateFix.exe, for both the installed and the
/// portable version. Only if that can't be written to (for example a
/// read-only folder) does it fall back to the user's app data folder.
fn choose_data_dir(app: &AppHandle) -> PathBuf {
    if let Some(dir) = data_dir_next_to_exe() {
        return dir;
    }
    let fallback = app
        .path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join(DATA_FOLDER));
    let _ = usable_dir(&fallback);
    fallback
}

fn journal_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.state::<DataDir>().0.join(UNDO_FILE))
}

fn summarise(outcomes: &[Outcome]) -> Summary {
    let mut summary = Summary {
        renamed: 0,
        skipped: 0,
        failed: 0,
        problems: Vec::new(),
    };
    for outcome in outcomes {
        match outcome {
            Outcome::Renamed { .. } => summary.renamed += 1,
            Outcome::Skipped { from, reason } => {
                summary.skipped += 1;
                summary
                    .problems
                    .push(format!("{}: {reason}", name_of(from)));
            }
            Outcome::Failed { from, error } => {
                summary.failed += 1;
                summary.problems.push(format!("{}: {error}", name_of(from)));
            }
        }
    }
    summary
}

#[tauri::command]
fn glass(state: State<Glass>) -> &'static str {
    state.0
}

/// Finds everything in `folder` that needs its date fixed. A dropped file
/// scans the folder it's in.
#[tauri::command]
fn scan(folder: PathBuf, recursive: bool, month_first: bool) -> Result<Scan, String> {
    let folder = if folder.is_file() {
        folder
            .parent()
            .map(Path::to_path_buf)
            .ok_or("That file has no folder")?
    } else {
        folder
    };
    if !folder.is_dir() {
        return Err(format!("{} isn't a folder", folder.display()));
    }
    let changes = plan(&folder, recursive, month_first).map_err(|e| e.to_string())?;
    let items = changes
        .into_iter()
        .map(|c| {
            let old_name = name_of(&c.from);
            let found = extract_date(&old_name, month_first);
            Item {
                old_date: found
                    .as_ref()
                    .map(|f| f.matched.clone())
                    .unwrap_or_default(),
                new_date: found.map(|f| f.iso).unwrap_or_default(),
                old_name,
                new_name: name_of(&c.to),
                location: c
                    .from
                    .parent()
                    .and_then(|p| p.strip_prefix(&folder).ok())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                is_dir: c.from.is_dir(),
                clash: c.clash,
                from: c.from,
                to: c.to,
            }
        })
        .collect();
    Ok(Scan { folder, items })
}

/// Renames the chosen items and remembers them so they can be undone.
#[tauri::command]
fn rename(
    app: AppHandle,
    folder: PathBuf,
    changes: Vec<Change>,
    on_clash: OnClash,
) -> Result<Summary, String> {
    let outcomes = apply(&changes, on_clash);
    if let Some(journal) = Journal::from_outcomes(&folder, &outcomes) {
        journal
            .save(&journal_path(&app)?)
            .map_err(|e| e.to_string())?;
    }
    Ok(summarise(&outcomes))
}

/// The last rename that can still be undone, if any.
#[tauri::command]
fn last_rename(app: AppHandle) -> Option<UndoInfo> {
    let journal = Journal::load(&journal_path(&app).ok()?)?;
    Some(UndoInfo {
        count: journal.renames.len(),
        folder: journal.folder,
        when: journal.when,
    })
}

/// Puts the names from the last rename back.
#[tauri::command]
fn undo(app: AppHandle) -> Result<Summary, String> {
    let path = journal_path(&app)?;
    let journal = Journal::load(&path).ok_or("There's nothing to undo")?;
    let outcomes = journal.undo();
    let _ = std::fs::remove_file(&path);
    Ok(summarise(&outcomes))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = choose_data_dir(app.handle());
            app.manage(DataDir(data_dir.clone()));

            // The window is created here rather than from the config, so the
            // browser engine's own files go in the same data folder too.
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .expect("main window in tauri.conf.json")
                .clone();
            let window = WebviewWindowBuilder::from_config(app.handle(), &config)?
                .data_directory(data_dir.join("webview"))
                .build()?;
            #[cfg(windows)]
            let effect = {
                use window_vibrancy::{apply_acrylic, apply_mica};
                if apply_mica(&window, Some(true)).is_ok() {
                    "mica"
                } else if apply_acrylic(&window, Some((16, 16, 28, 140))).is_ok() {
                    "acrylic"
                } else {
                    "none"
                }
            };
            #[cfg(not(windows))]
            let effect = {
                let _ = &window;
                "none"
            };
            app.manage(Glass(effect));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            glass,
            scan,
            rename,
            last_rename,
            undo
        ])
        .run(tauri::generate_context!())
        .expect("error while running DateFix");
}
