// No console window behind the app in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use datefix_core::{
    data_dir_next_to_exe, extract_date, plan_with, precheck, safe_to_undo, usable_dir, Batch,
    BatchState, Change, History, OnClash, Outcome, PlanOptions, Problem, Status, DATA_FOLDER,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindowBuilder};

/// The rename history, kept in the data folder.
struct Hist(History);

/// Set to stop the scan that's running.
#[derive(Default)]
struct ScanCancel(Arc<AtomicBool>);

/// How often progress is sent to the window.
const PROGRESS_EVERY: Duration = Duration::from_millis(80);

/// Windows paths longer than this can't be opened by many programs.
const MAX_PATH: usize = 259;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Source {
    path: PathBuf,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    from: PathBuf,
    to: PathBuf,
    old_name: String,
    new_name: String,
    /// Which source folder it came from.
    source: usize,
    /// Subfolder it's in, relative to its source ("" for the top).
    location: String,
    is_dir: bool,
    /// Something already has the new name.
    clash: bool,
    /// Another item in this scan wants the same new name.
    dupe: bool,
    /// The day and month could be read the other way round.
    ambiguous: bool,
    /// The new path is too long for many programs (and the old one wasn't).
    long_path: bool,
    /// It probably can't be renamed right now.
    problem: Option<Problem>,
    /// The date text in the old name, and the date as YYYY-MM-DD.
    old_date: String,
    iso: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Scan {
    sources: Vec<Source>,
    items: Vec<Item>,
    /// How many files and folders were looked at.
    checked: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ScanProgress {
    checked: usize,
    folders: usize,
    folder: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RenameProgress {
    done: usize,
    total: usize,
}

#[derive(Serialize, Default)]
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
struct BatchInfo {
    id: String,
    started: u64,
    finished: Option<u64>,
    folders: Vec<PathBuf>,
    /// Items in the plan.
    planned: usize,
    renamed: usize,
    undone: usize,
    state: BatchState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckInfo {
    index: usize,
    status: Status,
    is_dir: bool,
    /// The name it has now (or should have, if it can't be found).
    now_name: String,
    now: Option<PathBuf>,
    back_name: String,
    back_to: PathBuf,
    /// The folder it's in.
    folder: PathBuf,
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn batch_info(b: &Batch) -> BatchInfo {
    BatchInfo {
        id: b.id.clone(),
        started: b.started,
        finished: b.finished,
        folders: b.folders.clone(),
        planned: b.entries.len(),
        renamed: b.renamed(),
        undone: b.undone(),
        state: b.state(),
    }
}

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

fn summarise(outcomes: &[Outcome]) -> Summary {
    let mut summary = Summary::default();
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

/// Runs blocking work off the main thread, so the window stays responsive.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

/// Sends progress to the window at most every [`PROGRESS_EVERY`].
struct Throttle(Option<Instant>);

impl Throttle {
    fn due(&mut self) -> bool {
        let due = self.0.is_none_or(|last| last.elapsed() >= PROGRESS_EVERY);
        if due {
            self.0 = Some(Instant::now());
        }
        due
    }
}

/// Finds everything in `folders` that needs its date fixed. A dropped file
/// scans the folder it's in. Runs in the background and sends
/// `scan-progress` events; [`cancel_scan`] stops it.
#[tauri::command]
async fn scan(
    app: AppHandle,
    folders: Vec<PathBuf>,
    recursive: bool,
    month_first: bool,
    include_dirs: bool,
) -> Result<Scan, String> {
    let cancel = app.state::<ScanCancel>().0.clone();
    cancel.store(false, Ordering::SeqCst);
    let options = PlanOptions {
        recursive,
        month_first,
        include_dirs,
    };
    blocking(move || scan_folders(&app, folders, options, &cancel)).await
}

fn scan_folders(
    app: &AppHandle,
    folders: Vec<PathBuf>,
    options: PlanOptions,
    cancel: &AtomicBool,
) -> Result<Scan, String> {
    // Files become their folder; the same folder twice counts once.
    let mut sources: Vec<PathBuf> = Vec::new();
    for f in folders {
        let folder = if f.is_file() {
            f.parent()
                .map(Path::to_path_buf)
                .ok_or("That file has no folder")?
        } else {
            f
        };
        if !folder.is_dir() {
            return Err(format!("{} isn't a folder", folder.display()));
        }
        if !sources.contains(&folder) {
            sources.push(folder);
        }
    }

    let (mut checked, mut visited) = (0, 0);
    let mut throttle = Throttle(None);
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for (source, folder) in sources.iter().enumerate() {
        let mut visit = |dir: &Path, count: usize| {
            checked += count;
            visited += 1;
            if throttle.due() {
                let _ = app.emit(
                    "scan-progress",
                    ScanProgress {
                        checked,
                        folders: visited,
                        folder: dir.display().to_string(),
                    },
                );
            }
            !cancel.load(Ordering::SeqCst)
        };
        let changes = plan_with(folder, options, &mut visit).map_err(|e| {
            if e.kind() == std::io::ErrorKind::Interrupted {
                "cancelled".to_string()
            } else {
                format!("Couldn't read {}: {e}", folder.display())
            }
        })?;
        // Overlapping sources (a folder and one inside it) list items once.
        let changes: Vec<Change> = changes
            .into_iter()
            .filter(|c| seen.insert(c.from.clone()))
            .collect();
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                checked,
                folders: visited,
                folder: "checking which files are in use".into(),
            },
        );
        let problems = prechecks(&changes, cancel);
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        for (c, problem) in changes.into_iter().zip(problems) {
            let old_name = name_of(&c.from);
            let found = extract_date(&old_name, options.month_first);
            let long = |p: &Path| p.as_os_str().len() > MAX_PATH;
            items.push(Item {
                old_date: found
                    .as_ref()
                    .map(|f| f.matched.clone())
                    .unwrap_or_default(),
                iso: found.as_ref().map(|f| f.iso.clone()).unwrap_or_default(),
                ambiguous: found.is_some_and(|f| f.ambiguous),
                old_name,
                new_name: name_of(&c.to),
                source,
                location: c
                    .from
                    .parent()
                    .and_then(|p| p.strip_prefix(folder).ok())
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                is_dir: c.is_dir,
                clash: c.clash,
                dupe: false,
                long_path: long(&c.to) && !long(&c.from),
                problem,
                from: c.from,
                to: c.to,
            });
        }
    }

    // Two items that would get the same name (Windows ignores case).
    let mut wanted: HashMap<String, usize> = HashMap::new();
    for item in &items {
        *wanted
            .entry(item.to.to_string_lossy().to_lowercase())
            .or_default() += 1;
    }
    for item in &mut items {
        item.dupe = wanted[&item.to.to_string_lossy().to_lowercase()] > 1;
    }

    Ok(Scan {
        sources: sources
            .iter()
            .map(|p| Source {
                path: p.clone(),
                name: name_of(p),
            })
            .collect(),
        items,
        checked,
    })
}

/// Checks whether each item could be renamed now, several at a time:
/// opening files is slow, mostly because antivirus software looks at each one.
fn prechecks(changes: &[Change], cancel: &AtomicBool) -> Vec<Option<Problem>> {
    let threads = thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(16);
    let chunk = changes.len().div_ceil(threads).max(1);
    thread::scope(|s| {
        let workers: Vec<_> = changes
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .map(|c| {
                            if c.is_dir || cancel.load(Ordering::Relaxed) {
                                None
                            } else {
                                precheck(&c.from)
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .zip(changes.chunks(chunk))
            .flat_map(|(w, part)| w.join().unwrap_or_else(|_| vec![None; part.len()]))
            .collect()
    })
}

#[tauri::command]
fn cancel_scan(state: State<ScanCancel>) {
    state.0.store(true, Ordering::SeqCst);
}

/// Renames the chosen items. The plan is saved in the history first, so it
/// can be undone even after a crash; if that can't be saved, nothing is
/// renamed. Sends `rename-progress` events.
#[tauri::command]
async fn rename(
    app: AppHandle,
    folders: Vec<PathBuf>,
    changes: Vec<Change>,
    on_clash: OnClash,
) -> Result<Summary, String> {
    let history = app.state::<Hist>().0.clone();
    blocking(move || {
        let total = changes.len();
        let (mut done, mut throttle) = (0, Throttle(None));
        let progress = |_: &Outcome| {
            done += 1;
            if throttle.due() || done == total {
                let _ = app.emit("rename-progress", RenameProgress { done, total });
            }
        };
        let (_, outcomes) = history
            .run(&folders, &changes, |_| on_clash, progress)
            .map_err(|e| {
                format!("Nothing was renamed: the rename history couldn't be saved ({e})")
            })?;
        Ok(summarise(&outcomes))
    })
    .await
}

/// Every batch in the history, newest first.
#[tauri::command]
fn history_list(history: State<Hist>) -> Vec<BatchInfo> {
    history.0.list().iter().map(batch_info).collect()
}

fn get_batch(history: &History, id: &str) -> Result<Batch, String> {
    history
        .get(id)
        .ok_or_else(|| "That rename isn't in the history any more".to_string())
}

/// Checks each item in a batch against the disk, before an undo.
#[tauri::command]
async fn history_check(app: AppHandle, id: String) -> Result<Vec<CheckInfo>, String> {
    let history = app.state::<Hist>().0.clone();
    blocking(move || {
        let mut batch = get_batch(&history, &id)?;
        let checks = batch.check().map_err(|e| e.to_string())?;
        Ok(checks
            .into_iter()
            .map(|c| {
                let entry = &batch.entries[c.index];
                let shown = c
                    .now
                    .clone()
                    .or_else(|| entry.renamed_to.clone())
                    .unwrap_or_else(|| entry.to.clone());
                CheckInfo {
                    index: c.index,
                    status: c.status,
                    is_dir: entry.is_dir,
                    now_name: name_of(&shown),
                    now: c.now.clone(),
                    back_name: name_of(&c.back_to),
                    folder: c
                        .back_to
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_default(),
                    back_to: c.back_to,
                }
            })
            .collect())
    })
    .await
}

/// Undoes the chosen items of a batch (`None`: every item that's still the
/// same file). What can't be put back stays in the history.
#[tauri::command]
async fn history_undo(
    app: AppHandle,
    id: String,
    indices: Option<Vec<usize>>,
) -> Result<Summary, String> {
    let history = app.state::<Hist>().0.clone();
    blocking(move || {
        let mut batch = get_batch(&history, &id)?;
        let outcomes = match indices {
            Some(chosen) => {
                let chosen: HashSet<usize> = chosen.into_iter().collect();
                batch.undo(|c| chosen.contains(&c.index))
            }
            None => batch.undo(safe_to_undo),
        }
        .map_err(|e| e.to_string())?;
        Ok(summarise(&outcomes))
    })
    .await
}

/// Finishes an interrupted batch.
#[tauri::command]
async fn history_resume(app: AppHandle, id: String, on_clash: OnClash) -> Result<Summary, String> {
    let history = app.state::<Hist>().0.clone();
    blocking(move || {
        let mut batch = get_batch(&history, &id)?;
        let outcomes = batch.resume(on_clash).map_err(|e| e.to_string())?;
        Ok(summarise(&outcomes))
    })
    .await
}

/// Deletes a batch from the history (the files aren't touched).
#[tauri::command]
fn history_forget(history: State<Hist>, id: String) -> Result<(), String> {
    history.0.forget(&id).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ScanCancel::default())
        .setup(|app| {
            let data_dir = choose_data_dir(app.handle());
            app.manage(Hist(History::open(&data_dir)));

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
            #[allow(unused_mut)]
            let mut window = WebviewWindowBuilder::from_config(app.handle(), &config)?
                .data_directory(data_dir.join("webview"));
            // Debug builds only: DATEFIX_DEBUG_PORT=9333 opens the WebView2
            // debugging port, so tests can drive the real window. (Setting
            // WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS instead clashes with
            // Tauri's own arguments.) The first part repeats Tauri's defaults.
            #[cfg(debug_assertions)]
            if let Ok(port) = std::env::var("DATEFIX_DEBUG_PORT") {
                window = window.additional_browser_args(&format!(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
                ));
            }
            window.build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan,
            cancel_scan,
            rename,
            history_list,
            history_check,
            history_undo,
            history_resume,
            history_forget
        ])
        .run(tauri::generate_context!())
        .expect("error while running DateFix");
}
