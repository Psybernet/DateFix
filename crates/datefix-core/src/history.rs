//! Rename history: every batch of renames is written down before anything is
//! renamed, so it can be undone later, even after a crash or a restart.
//!
//! Each batch is one file in `DateFix data\history`, one JSON record per line:
//!
//! 1. a `batch` header and a `plan` line for every item, with a fingerprint
//!    of each file. These are flushed to disk before the first rename.
//! 2. a `done` line after each rename, as it happens.
//! 3. a `finished` line at the end. A batch without it was interrupted.
//! 4. later, an `undone` line for each name that was put back.
//!
//! Lines are only ever added, so a crash can at worst cut off the last line,
//! which is ignored. If a crash loses `done` lines, the files are found again
//! by their fingerprints.

use crate::files::{numbered, rename_one, Change, OnClash, Outcome};
use crate::undo::Journal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The folder inside "DateFix data" that holds the history.
pub const HISTORY_FOLDER: &str = "history";

/// How many finished batches to keep. Interrupted batches are always kept.
pub const KEEP_BATCHES: usize = 30;

/// How much of the start and the end of a file goes into its fingerprint.
const EDGE: u64 = 64 * 1024;

/// How often, in renames, the history is forced out to the disk.
const SYNC_EVERY: usize = 64;

/// The largest `_n` looked at when finding a file after a crash.
const MAX_NUMBER: u32 = 1000;

/// Recognises a file again later, even under another name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub size: u64,
    /// SHA-256 of the size and the first and last 64 KiB (the whole file if
    /// it's 128 KiB or smaller). Fast for large photos and videos, and enough
    /// to tell files apart.
    pub sha256: String,
}

/// The fingerprint of a file, or `None` for a folder.
pub fn fingerprint(path: &Path) -> io::Result<Option<Fingerprint>> {
    let meta = fs::metadata(path)?;
    if meta.is_dir() {
        return Ok(None);
    }
    let size = meta.len();
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    hasher.update(size.to_le_bytes());
    if size <= 2 * EDGE {
        let mut all = Vec::with_capacity(size as usize);
        file.read_to_end(&mut all)?;
        hasher.update(&all);
    } else {
        let mut buf = vec![0; EDGE as usize];
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
        file.seek(SeekFrom::End(-(EDGE as i64)))?;
        file.read_exact(&mut buf)?;
        hasher.update(&buf);
    }
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok(Some(Fingerprint { size, sha256 }))
}

/// One line in a history file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Record {
    Batch {
        version: u32,
        id: String,
        started: u64,
        folders: Vec<PathBuf>,
    },
    Plan {
        i: usize,
        from: PathBuf,
        to: PathBuf,
        is_dir: bool,
        #[serde(default)]
        fingerprint: Option<Fingerprint>,
    },
    Done {
        i: usize,
        to: PathBuf,
    },
    Finished {
        at: u64,
    },
    Undone {
        i: usize,
    },
}

/// One item in a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub from: PathBuf,
    /// The name it was planned to get.
    pub to: PathBuf,
    pub is_dir: bool,
    /// `None` for folders, files that couldn't be read, and history from
    /// before 2.2.
    pub fingerprint: Option<Fingerprint>,
    /// Where it was renamed to (with `_2` if that was added), if it was.
    pub renamed_to: Option<PathBuf>,
    /// The old name has been put back.
    pub undone: bool,
}

/// Where a batch stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    /// DateFix stopped before the batch was finished.
    Interrupted,
    /// Finished, and nothing has been undone.
    Complete,
    /// Some names have been put back.
    PartlyUndone,
    /// Every name has been put back.
    Undone,
}

/// What undo would do with one item, checked against the disk right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Still has its new name and the same contents.
    Matches,
    /// Renamed again since, and found by its fingerprint.
    RenamedSince,
    /// Has its new name, but its contents changed (or can't be read).
    ContentsChanged,
    /// Can't be found.
    Missing,
    /// Something else now has the old name.
    OldNameTaken,
    /// Already has its old name back.
    AlreadyBack,
    /// Was never renamed (skipped, failed, or not reached).
    NotRenamed,
    /// Its old name has already been put back by DateFix.
    Undone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// The item's place in the batch.
    pub index: usize,
    pub status: Status,
    /// Where the item is now, if it was found.
    pub now: Option<PathBuf>,
    /// The name undo would give back.
    pub back_to: PathBuf,
}

/// The items undo handles unless asked otherwise: those that are still the
/// same file. Items whose contents changed are only undone when chosen.
pub fn safe_to_undo(check: &Check) -> bool {
    matches!(check.status, Status::Matches | Status::RenamedSince)
}

/// One batch of renames.
#[derive(Debug, Clone)]
pub struct Batch {
    pub id: String,
    /// When it started, in seconds since 1970.
    pub started: u64,
    /// The folders that were scanned.
    pub folders: Vec<PathBuf>,
    pub entries: Vec<Entry>,
    /// When it finished, or `None` if it was interrupted.
    pub finished: Option<u64>,
    file: PathBuf,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Writes one record as one line, in a single write.
fn append(file: &mut File, record: &Record) -> io::Result<()> {
    let mut line = serde_json::to_string(record).map_err(io::Error::other)?;
    line.push('\n');
    file.write_all(line.as_bytes())
}

impl Batch {
    /// Reads a history file. A line that can't be read (such as one cut off
    /// by a crash) is ignored.
    pub fn load(file: &Path) -> Option<Batch> {
        let text = fs::read_to_string(file).ok()?;
        let mut lines = text.lines().filter_map(|l| serde_json::from_str(l).ok());
        let Some(Record::Batch {
            id,
            started,
            folders,
            ..
        }) = lines.next()
        else {
            return None;
        };
        let mut batch = Batch {
            id,
            started,
            folders,
            entries: Vec::new(),
            finished: None,
            file: file.to_path_buf(),
        };
        for record in lines {
            match record {
                Record::Plan {
                    i,
                    from,
                    to,
                    is_dir,
                    fingerprint,
                } if i == batch.entries.len() => batch.entries.push(Entry {
                    from,
                    to,
                    is_dir,
                    fingerprint,
                    renamed_to: None,
                    undone: false,
                }),
                Record::Done { i, to } => {
                    if let Some(e) = batch.entries.get_mut(i) {
                        e.renamed_to = Some(to);
                    }
                }
                Record::Undone { i } => {
                    if let Some(e) = batch.entries.get_mut(i) {
                        e.undone = true;
                    }
                }
                Record::Finished { at } => batch.finished = Some(at),
                _ => {}
            }
        }
        Some(batch)
    }

    /// How many items were renamed.
    pub fn renamed(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.renamed_to.is_some())
            .count()
    }

    /// How many renamed items have their old name back.
    pub fn undone(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.renamed_to.is_some() && e.undone)
            .count()
    }

    pub fn state(&self) -> BatchState {
        let (renamed, undone) = (self.renamed(), self.undone());
        if renamed > 0 && undone == renamed {
            BatchState::Undone
        } else if self.finished.is_none() {
            BatchState::Interrupted
        } else if undone > 0 {
            BatchState::PartlyUndone
        } else {
            BatchState::Complete
        }
    }

    /// Something in this batch still has its new name.
    pub fn can_undo(&self) -> bool {
        self.renamed() > self.undone()
    }

    fn open_log(&self) -> io::Result<File> {
        OpenOptions::new().append(true).open(&self.file)
    }

    /// Where `path` (as written in the history) is now, after the folders
    /// renamed in this batch. Folders come after their contents in a batch,
    /// so applying them in order also handles folders inside folders.
    fn current(&self, path: &Path) -> PathBuf {
        let mut path = path.to_path_buf();
        for e in &self.entries {
            let Some(to) = e.renamed_to.as_ref().filter(|_| e.is_dir && !e.undone) else {
                continue;
            };
            if let Ok(rest) = path.strip_prefix(&e.from) {
                if !rest.as_os_str().is_empty() {
                    path = to.join(rest);
                }
            }
        }
        path
    }

    /// `path` holds the same item that `entry` was.
    fn same(entry: &Entry, path: &Path) -> bool {
        if entry.is_dir {
            return path.is_dir();
        }
        match &entry.fingerprint {
            Some(fp) => fingerprint(path).ok().flatten().as_ref() == Some(fp),
            None => path.is_file(),
        }
    }

    /// Looks for a file renamed again since, in the folder it was left in.
    fn find_moved(entry: &Entry, expected: &Path) -> Option<PathBuf> {
        let fp = entry.fingerprint.as_ref().filter(|_| !entry.is_dir)?;
        fs::read_dir(expected.parent()?)
            .ok()?
            .flatten()
            .filter(|f| {
                f.metadata()
                    .is_ok_and(|m| m.is_file() && m.len() == fp.size)
            })
            .map(|f| f.path())
            .find(|p| p != expected && Self::same(entry, p))
    }

    /// Checks one item against the disk.
    fn check_one(&self, index: usize) -> Check {
        let e = &self.entries[index];
        let back_to = self.current(&e.from);
        let result = |status, now| Check {
            index,
            status,
            now,
            back_to: back_to.clone(),
        };
        if e.undone {
            return result(Status::Undone, None);
        }
        let Some(renamed) = &e.renamed_to else {
            return result(Status::NotRenamed, None);
        };
        let to = self.current(renamed);
        let to_there = fs::symlink_metadata(&to).is_ok();
        if fs::symlink_metadata(&back_to).is_ok() {
            return if !to_there && Self::same(e, &back_to) {
                result(Status::AlreadyBack, Some(back_to.clone()))
            } else {
                result(Status::OldNameTaken, to_there.then_some(to))
            };
        }
        if to_there {
            let status = if Self::same(e, &to) {
                Status::Matches
            } else {
                Status::ContentsChanged
            };
            return result(status, Some(to));
        }
        match Self::find_moved(e, &to) {
            Some(path) => result(Status::RenamedSince, Some(path)),
            None => result(Status::Missing, None),
        }
    }

    /// After a crash, the last few `done` lines may not have reached the
    /// disk. Finds those renames by looking for the files under their new
    /// names, and writes them down.
    fn recover(&mut self) -> io::Result<()> {
        if self.finished.is_some() {
            return Ok(());
        }
        let mut found = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if e.renamed_to.is_some() || e.undone || self.current(&e.from).exists() {
                continue;
            }
            for n in 1..=MAX_NUMBER {
                let candidate = if n == 1 {
                    e.to.clone()
                } else {
                    numbered(&e.to, n)
                };
                let path = self.current(&candidate);
                if !path.exists() {
                    if n == 1 {
                        continue;
                    }
                    break;
                }
                if Self::same(e, &path) {
                    found.push((i, candidate));
                    break;
                }
            }
        }
        if found.is_empty() {
            return Ok(());
        }
        let mut log = self.open_log()?;
        for (i, to) in found {
            append(&mut log, &Record::Done { i, to: to.clone() })?;
            self.entries[i].renamed_to = Some(to);
        }
        log.sync_data()
    }

    /// Checks every item against the disk, for showing before an undo.
    pub fn check(&mut self) -> io::Result<Vec<Check>> {
        self.recover()?;
        Ok((0..self.entries.len()).map(|i| self.check_one(i)).collect())
    }

    /// Puts names back, newest rename first, so folders get their old names
    /// back before the items inside them. `include` chooses which items to
    /// undo; pass [`safe_to_undo`] for the usual choice. Every name put back
    /// is written down, so an undo that's interrupted can be carried on.
    pub fn undo(&mut self, include: impl Fn(&Check) -> bool) -> io::Result<Vec<Outcome>> {
        self.recover()?;
        let mut log = self.open_log()?;
        let mut outcomes = Vec::new();
        for i in (0..self.entries.len()).rev() {
            let check = self.check_one(i);
            // Where it is now, or failing that, where it should be.
            let from = check
                .now
                .clone()
                .or_else(|| self.entries[i].renamed_to.as_ref().map(|p| self.current(p)))
                .unwrap_or_else(|| check.back_to.clone());
            let skipped = |reason: &str| Outcome::Skipped {
                from: from.clone(),
                reason: reason.into(),
            };
            let outcome = match check.status {
                Status::NotRenamed | Status::Undone => continue,
                Status::AlreadyBack => {
                    append(&mut log, &Record::Undone { i })?;
                    self.entries[i].undone = true;
                    continue;
                }
                Status::Missing => Outcome::Failed {
                    from: from.clone(),
                    error: "it has been moved or deleted since".into(),
                },
                Status::OldNameTaken => skipped("something else now has the old name"),
                Status::Matches | Status::RenamedSince | Status::ContentsChanged
                    if include(&check) =>
                {
                    match fs::rename(&from, &check.back_to) {
                        Ok(()) => {
                            append(&mut log, &Record::Undone { i })?;
                            self.entries[i].undone = true;
                            Outcome::Renamed {
                                from: from.clone(),
                                to: check.back_to.clone(),
                            }
                        }
                        Err(e) => Outcome::Failed {
                            from: from.clone(),
                            error: e.to_string(),
                        },
                    }
                }
                Status::ContentsChanged => skipped("its contents changed after it was renamed"),
                Status::Matches | Status::RenamedSince => skipped("left out"),
            };
            outcomes.push(outcome);
        }
        log.sync_data()?;
        Ok(outcomes)
    }

    /// Carries on an interrupted batch: renames the items it didn't get to.
    pub fn resume(&mut self, on_clash: OnClash) -> io::Result<Vec<Outcome>> {
        self.recover()?;
        let mut log = self.open_log()?;
        let mut outcomes = Vec::new();
        for i in 0..self.entries.len() {
            let e = &self.entries[i];
            if e.renamed_to.is_some() || e.undone || self.finished.is_some() {
                continue;
            }
            let outcome = rename_one(&self.current(&e.from), &self.current(&e.to), on_clash);
            if let Outcome::Renamed { to, .. } = &outcome {
                append(&mut log, &Record::Done { i, to: to.clone() })?;
                self.entries[i].renamed_to = Some(to.clone());
            }
            outcomes.push(outcome);
        }
        let at = now_secs();
        append(&mut log, &Record::Finished { at })?;
        log.sync_all()?;
        self.finished = Some(at);
        Ok(outcomes)
    }
}

/// All the batches kept in one history folder.
#[derive(Debug, Clone)]
pub struct History {
    dir: PathBuf,
}

impl History {
    /// The history in `data_dir` (the "DateFix data" folder). Undo history
    /// from before 2.2 (`last-rename.json`) is moved into it.
    pub fn open(data_dir: &Path) -> History {
        let history = History {
            dir: data_dir.join(HISTORY_FOLDER),
        };
        let legacy = data_dir.join(crate::UNDO_FILE);
        if let Some(journal) = Journal::load(&legacy) {
            if history.import(&journal).is_ok() {
                let _ = fs::remove_file(&legacy);
            }
        }
        history
    }

    /// The folder the batches are kept in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every batch, newest first.
    pub fn list(&self) -> Vec<Batch> {
        let Ok(files) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut batches: Vec<Batch> = files
            .flatten()
            .map(|f| f.path())
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
            .filter_map(|p| Batch::load(&p))
            .collect();
        batches.sort_by(|a, b| b.id.cmp(&a.id));
        batches
    }

    pub fn get(&self, id: &str) -> Option<Batch> {
        self.list().into_iter().find(|b| b.id == id)
    }

    /// The newest batch that still has something to undo.
    pub fn latest_undoable(&self) -> Option<Batch> {
        self.list().into_iter().find(Batch::can_undo)
    }

    /// Batches that were cut off by a crash or by DateFix being closed.
    pub fn interrupted(&self) -> Vec<Batch> {
        self.list()
            .into_iter()
            .filter(|b| b.finished.is_none())
            .collect()
    }

    /// Deletes a batch from the history.
    pub fn forget(&self, id: &str) -> io::Result<()> {
        match self.get(id) {
            Some(batch) => fs::remove_file(batch.file),
            None => Ok(()),
        }
    }

    /// Keeps the newest `keep` finished batches. Interrupted ones are kept.
    pub fn prune(&self, keep: usize) {
        let finished = self.list().into_iter().filter(|b| b.finished.is_some());
        for old in finished.skip(keep) {
            let _ = fs::remove_file(old.file);
        }
    }

    /// Creates a new, empty history file with a unique id based on the time.
    fn new_file(&self, started: SystemTime) -> io::Result<(String, PathBuf, File)> {
        fs::create_dir_all(&self.dir)?;
        let since = started.duration_since(UNIX_EPOCH).unwrap_or_default();
        let base = chrono::DateTime::from_timestamp(since.as_secs() as i64, since.subsec_nanos())
            .unwrap_or_default()
            .format("%Y%m%d-%H%M%S-%3f")
            .to_string();
        for n in 1.. {
            let id = if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let path = self.dir.join(format!("{id}.jsonl"));
            match OpenOptions::new().append(true).create_new(true).open(&path) {
                Ok(file) => return Ok((id, path, file)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    /// Renames `changes` in order and keeps a history so it can be undone.
    ///
    /// The plan is saved to disk first: if that fails, nothing is renamed and
    /// the error is returned. `on_clash` decides, for each item whose new
    /// name is taken, whether to skip it or add a number. `progress` is told
    /// about each item as it's done. Returns the batch id and what happened.
    pub fn run(
        &self,
        folders: &[PathBuf],
        changes: &[Change],
        mut on_clash: impl FnMut(&Change) -> OnClash,
        mut progress: impl FnMut(&Outcome),
    ) -> io::Result<(String, Vec<Outcome>)> {
        let (id, path, mut log) = self.new_file(SystemTime::now())?;
        let write_plan = |log: &mut File| -> io::Result<()> {
            append(
                log,
                &Record::Batch {
                    version: 1,
                    id: id.clone(),
                    started: now_secs(),
                    folders: folders.to_vec(),
                },
            )?;
            for (i, c) in changes.iter().enumerate() {
                append(
                    log,
                    &Record::Plan {
                        i,
                        from: c.from.clone(),
                        to: c.to.clone(),
                        is_dir: c.from.is_dir(),
                        // A file that can't be read (one that's in use, say)
                        // is still renamed; undo then goes by its name alone.
                        fingerprint: fingerprint(&c.from).ok().flatten(),
                    },
                )?;
            }
            log.sync_all()
        };
        if let Err(e) = write_plan(&mut log) {
            drop(log);
            let _ = fs::remove_file(&path);
            return Err(e);
        }

        let mut outcomes = Vec::with_capacity(changes.len());
        let mut broken: Option<io::Error> = None;
        for (i, change) in changes.iter().enumerate() {
            let outcome = if let Some(e) = &broken {
                // Don't rename what can't be written down.
                Outcome::Skipped {
                    from: change.from.clone(),
                    reason: format!("the rename history couldn't be saved ({e})"),
                }
            } else {
                let outcome = rename_one(&change.from, &change.to, on_clash(change));
                if let Outcome::Renamed { to, .. } = &outcome {
                    if let Err(e) = append(&mut log, &Record::Done { i, to: to.clone() }) {
                        broken = Some(e);
                    }
                }
                if (i + 1) % SYNC_EVERY == 0 {
                    let _ = log.sync_data();
                }
                outcome
            };
            progress(&outcome);
            outcomes.push(outcome);
        }
        let _ = append(&mut log, &Record::Finished { at: now_secs() });
        let _ = log.sync_all();
        drop(log);

        if !outcomes.iter().any(Outcome::is_renamed) {
            let _ = fs::remove_file(&path);
        }
        self.prune(KEEP_BATCHES);
        Ok((id, outcomes))
    }

    /// Adds a batch from the old single-batch undo file.
    fn import(&self, journal: &Journal) -> io::Result<()> {
        let started = UNIX_EPOCH + std::time::Duration::from_secs(journal.when);
        let (id, path, mut log) = self.new_file(started)?;
        let result = (|| {
            append(
                &mut log,
                &Record::Batch {
                    version: 1,
                    id,
                    started: journal.when,
                    folders: vec![journal.folder.clone()],
                },
            )?;
            for (i, r) in journal.renames.iter().enumerate() {
                append(
                    &mut log,
                    &Record::Plan {
                        i,
                        from: r.from.clone(),
                        to: r.to.clone(),
                        is_dir: r.to.is_dir(),
                        fingerprint: None,
                    },
                )?;
                append(
                    &mut log,
                    &Record::Done {
                        i,
                        to: r.to.clone(),
                    },
                )?;
            }
            append(&mut log, &Record::Finished { at: journal.when })?;
            log.sync_all()
        })();
        if result.is_err() {
            drop(log);
            let _ = fs::remove_file(&path);
        }
        result
    }
}
