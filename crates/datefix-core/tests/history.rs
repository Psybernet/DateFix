//! The rename history: undo across restarts, after crashes, and after files
//! have been renamed, changed or deleted since.

use datefix_core::*;
use std::fs;
use std::path::{Path, PathBuf};

struct Setup {
    _tmp: tempfile::TempDir,
    files: PathBuf,
    data: PathBuf,
}

fn setup(names: &[&str]) -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let files = tmp.path().join("files");
    let data = tmp.path().join("DateFix data");
    for name in names {
        let path = files.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Different contents for each file, so fingerprints differ.
        fs::write(&path, format!("contents of {name}")).unwrap();
    }
    fs::create_dir_all(&files).unwrap();
    Setup {
        _tmp: tmp,
        files,
        data,
    }
}

/// Plans and renames everything in the folder (with subfolders).
fn run(s: &Setup) -> (History, String) {
    let history = History::open(&s.data);
    let changes = plan(&s.files, true, false).unwrap();
    let (id, outcomes) = history
        .run(&[s.files.clone()], &changes, |_| OnClash::Number, |_| {})
        .unwrap();
    assert!(outcomes.iter().all(Outcome::is_renamed), "{outcomes:?}");
    (history, id)
}

/// Rewrites the batch's history file, as a crash might have left it.
fn edit_log(history: &History, edit: impl FnOnce(&mut Vec<String>)) {
    let file = fs::read_dir(history.dir())
        .unwrap()
        .flatten()
        .map(|f| f.path())
        .find(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .unwrap();
    let text = fs::read_to_string(&file).unwrap();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    edit(&mut lines);
    fs::write(&file, lines.join("\n") + "\n").unwrap();
}

/// Removes the `finished` line and the last `n` `done` lines.
fn crash(history: &History, n: usize) {
    edit_log(history, |lines| {
        lines.retain(|l| !l.contains(r#""kind":"finished""#));
        for _ in 0..n {
            let last = lines
                .iter()
                .rposition(|l| l.contains(r#""kind":"done""#))
                .unwrap();
            lines.remove(last);
        }
    });
}

fn names(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        out.push(path.file_name().unwrap().to_string_lossy().into_owned());
        if path.is_dir() {
            for inner in names(&path) {
                out.push(format!(
                    "{}/{inner}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn undo_works_after_a_restart() {
    let s = setup(&[
        "Trip 5 June 2023/photo 20230605.jpg",
        "Report Nov 22 2022.pdf",
    ]);
    let before = names(&s.files);
    let (_, id) = run(&s);
    assert_ne!(names(&s.files), before);

    // A new History is what DateFix sees after being closed and reopened.
    let mut batch = History::open(&s.data).latest_undoable().unwrap();
    assert_eq!(batch.id, id);
    assert_eq!(batch.state(), BatchState::Complete);
    assert_eq!(batch.renamed(), 3);
    assert!(batch
        .check()
        .unwrap()
        .iter()
        .all(|c| c.status == Status::Matches));

    let outcomes = batch.undo(safe_to_undo).unwrap();
    assert_eq!(outcomes.len(), 3);
    assert!(outcomes.iter().all(Outcome::is_renamed));
    assert_eq!(names(&s.files), before);

    // The batch is kept, marked as undone, and there's nothing left to undo.
    let history = History::open(&s.data);
    assert_eq!(history.get(&id).unwrap().state(), BatchState::Undone);
    assert!(history.latest_undoable().is_none());
}

#[test]
fn a_crash_that_lost_the_last_lines_is_recovered_by_fingerprint() {
    let s = setup(&[
        "a 01.02.2020.txt",
        "b 02.03.2021.txt",
        "c 03.04.2022.txt",
        "d 04.05.2023.txt",
    ]);
    let before = names(&s.files);
    let (history, _) = run(&s);
    crash(&history, 2);

    let mut batch = history.interrupted().pop().unwrap();
    assert_eq!(batch.state(), BatchState::Interrupted);
    assert_eq!(batch.renamed(), 2, "two renames were lost from the history");

    // Checking finds them on disk and writes them back into the history.
    assert!(batch
        .check()
        .unwrap()
        .iter()
        .all(|c| c.status == Status::Matches));
    assert_eq!(history.interrupted()[0].renamed(), 4);

    batch.undo(safe_to_undo).unwrap();
    assert_eq!(names(&s.files), before);
}

#[test]
fn a_lost_rename_that_got_a_number_is_recovered() {
    let s = setup(&["a 2020-02-01.txt", "a 01.02.2020.txt"]);
    let (history, _) = run(&s);
    assert!(s.files.join("a 2020-02-01_2.txt").exists());
    crash(&history, 1);

    let mut batch = history.interrupted().pop().unwrap();
    assert_eq!(batch.check().unwrap()[0].status, Status::Matches);
    batch.undo(safe_to_undo).unwrap();
    assert_eq!(names(&s.files), ["a 01.02.2020.txt", "a 2020-02-01.txt"]);
}

#[test]
fn an_interrupted_batch_can_be_finished() {
    let s = setup(&["a 01.02.2020.txt", "b 02.03.2021.txt", "c 03.04.2022.txt"]);
    let (history, _) = run(&s);
    // As if DateFix stopped before renaming the last two.
    for (new, old) in [
        ("b 2021-03-02.txt", "b 02.03.2021.txt"),
        ("c 2022-04-03.txt", "c 03.04.2022.txt"),
    ] {
        fs::rename(s.files.join(new), s.files.join(old)).unwrap();
    }
    crash(&history, 2);

    let mut batch = history.interrupted().pop().unwrap();
    let outcomes = batch.resume(OnClash::Number).unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(Outcome::is_renamed));
    assert_eq!(
        names(&s.files),
        ["a 2020-02-01.txt", "b 2021-03-02.txt", "c 2022-04-03.txt"]
    );
    let batch = history.list().pop().unwrap();
    assert_eq!(batch.state(), BatchState::Complete);
    assert_eq!(batch.renamed(), 3);
}

#[test]
fn a_file_renamed_again_since_is_found_and_undone() {
    let s = setup(&["scan 05.11.2023.txt"]);
    let (history, _) = run(&s);
    fs::rename(
        s.files.join("scan 2023-11-05.txt"),
        s.files.join("scan 2023-11-05 final.txt"),
    )
    .unwrap();

    let mut batch = history.latest_undoable().unwrap();
    let checks = batch.check().unwrap();
    assert_eq!(checks[0].status, Status::RenamedSince);
    assert_eq!(
        checks[0].now.as_deref(),
        Some(s.files.join("scan 2023-11-05 final.txt").as_path())
    );

    batch.undo(safe_to_undo).unwrap();
    assert_eq!(names(&s.files), ["scan 05.11.2023.txt"]);
    assert_eq!(
        fs::read_to_string(s.files.join("scan 05.11.2023.txt")).unwrap(),
        "contents of scan 05.11.2023.txt"
    );
}

#[test]
fn a_changed_file_is_only_undone_when_chosen() {
    let s = setup(&["notes 5 June 2023.txt"]);
    let (history, _) = run(&s);
    fs::write(s.files.join("notes 2023-06-05.txt"), "edited since").unwrap();

    let mut batch = history.latest_undoable().unwrap();
    assert_eq!(batch.check().unwrap()[0].status, Status::ContentsChanged);
    let outcomes = batch.undo(safe_to_undo).unwrap();
    assert!(matches!(outcomes[0], Outcome::Skipped { .. }));
    assert_eq!(names(&s.files), ["notes 2023-06-05.txt"]);

    let outcomes = batch.undo(|_| true).unwrap();
    assert!(outcomes[0].is_renamed());
    assert_eq!(names(&s.files), ["notes 5 June 2023.txt"]);
}

#[test]
fn a_missing_file_keeps_its_history_for_later() {
    let s = setup(&["a 01.02.2020.txt", "b 02.03.2021.txt"]);
    let (history, id) = run(&s);
    let moved = s._tmp.path().join("moved away.txt");
    fs::rename(s.files.join("b 2021-03-02.txt"), &moved).unwrap();

    let mut batch = history.latest_undoable().unwrap();
    let outcomes = batch.undo(safe_to_undo).unwrap();
    assert_eq!(outcomes.iter().filter(|o| o.is_renamed()).count(), 1);
    assert!(outcomes.iter().any(|o| matches!(o, Outcome::Failed { .. })));

    // Not forgotten: put the file back and undo again.
    let mut batch = history.get(&id).unwrap();
    assert_eq!(batch.state(), BatchState::PartlyUndone);
    assert!(batch.can_undo());
    fs::rename(&moved, s.files.join("b 2021-03-02.txt")).unwrap();
    let outcomes = batch.undo(safe_to_undo).unwrap();
    assert!(outcomes.iter().all(Outcome::is_renamed));
    assert_eq!(names(&s.files), ["a 01.02.2020.txt", "b 02.03.2021.txt"]);
    assert_eq!(history.get(&id).unwrap().state(), BatchState::Undone);
}

#[test]
fn undo_never_overwrites_something_with_the_old_name() {
    let s = setup(&["a 01.02.2020.txt"]);
    let (history, _) = run(&s);
    fs::write(s.files.join("a 01.02.2020.txt"), "a new file").unwrap();

    let mut batch = history.latest_undoable().unwrap();
    assert_eq!(batch.check().unwrap()[0].status, Status::OldNameTaken);
    let outcomes = batch.undo(|_| true).unwrap();
    assert!(matches!(outcomes[0], Outcome::Skipped { .. }));
    assert_eq!(
        fs::read_to_string(s.files.join("a 01.02.2020.txt")).unwrap(),
        "a new file"
    );
    assert!(s.files.join("a 2020-02-01.txt").exists());
}

#[test]
fn folders_inside_folders_are_checked_and_undone() {
    let s = setup(&["Trip 1 Jan 2020/Day 2 Feb 2021/photo 3 Mar 2022.txt"]);
    let before = names(&s.files);
    let (history, _) = run(&s);
    assert!(s
        .files
        .join("Trip 2020-01-01/Day 2021-02-02/photo 2022-03-03.txt")
        .exists());
    // Lose the outer folder's rename from the history, as a crash might.
    crash(&history, 1);

    let mut batch = history.interrupted().pop().unwrap();
    let checks = batch.check().unwrap();
    assert!(
        checks.iter().all(|c| c.status == Status::Matches),
        "{checks:?}"
    );
    batch.undo(safe_to_undo).unwrap();
    assert_eq!(names(&s.files), before);
}

#[test]
fn a_line_cut_off_by_a_crash_is_ignored() {
    let s = setup(&["a 01.02.2020.txt"]);
    let (history, id) = run(&s);
    edit_log(&history, |lines| {
        lines.push(r#"{"kind":"undone","i"#.into())
    });
    let batch = history.get(&id).unwrap();
    assert_eq!(batch.state(), BatchState::Complete);
    assert!(batch.can_undo());
}

#[test]
fn nothing_is_renamed_if_the_history_cannot_be_saved() {
    let s = setup(&["a 01.02.2020.txt"]);
    // A file where the history folder should be.
    fs::create_dir_all(&s.data).unwrap();
    fs::write(s.data.join(HISTORY_FOLDER), "in the way").unwrap();

    let history = History::open(&s.data);
    let changes = plan(&s.files, false, false).unwrap();
    let result = history.run(&[s.files.clone()], &changes, |_| OnClash::Number, |_| {});
    assert!(result.is_err());
    assert_eq!(names(&s.files), ["a 01.02.2020.txt"]);
}

#[test]
fn undo_history_from_before_2_2_is_imported() {
    let s = setup(&["a 01.02.2020.txt"]);
    let outcomes = apply(&plan(&s.files, false, false).unwrap(), OnClash::Number);
    let old = s.data.join(UNDO_FILE);
    Journal::from_outcomes(&s.files, &outcomes)
        .unwrap()
        .save(&old)
        .unwrap();

    let history = History::open(&s.data);
    assert!(!old.exists(), "the old file is moved into the history");
    let mut batch = history.latest_undoable().unwrap();
    assert_eq!(batch.renamed(), 1);
    batch.undo(safe_to_undo).unwrap();
    assert_eq!(names(&s.files), ["a 01.02.2020.txt"]);
}

#[test]
fn pruning_keeps_the_newest_and_every_interrupted_batch() {
    let s = setup(&[]);
    let history = History::open(&s.data);
    let mut ids = Vec::new();
    for n in 1..=4 {
        let name = format!("file {n}.0{n}.2020.txt");
        fs::write(s.files.join(&name), &name).unwrap();
        let changes = plan(&s.files, false, false).unwrap();
        let (id, _) = history
            .run(&[s.files.clone()], &changes, |_| OnClash::Number, |_| {})
            .unwrap();
        ids.push(id);
        if n == 1 {
            crash(&history, 0); // the oldest batch was interrupted
        }
    }
    history.prune(2);
    let kept: Vec<String> = history.list().into_iter().map(|b| b.id).collect();
    assert_eq!(kept, [ids[3].clone(), ids[2].clone(), ids[0].clone()]);
}
