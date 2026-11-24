use datefix_core::*;
use std::fs;

#[test]
fn renames_supported_formats() {
    let cases = [
        // Numbers, year first
        ("2023-11-5 notes", "2023-11-05 notes"),
        ("2023_11_05 photo", "2023-11-05 photo"),
        ("2023.11.05 scan", "2023-11-05 scan"),
        // Numbers, day first
        ("05-11-2023 report", "2023-11-05 report"),
        ("5.11.2023.pdf", "2023-11-05.pdf"),
        // Day, month name, year
        ("11 November 2022 notes.txt", "2022-11-11 notes.txt"),
        ("5 June 2023", "2023-06-05"),
        ("3rd July 2021 trip", "2021-07-03 trip"),
        ("11Nov23", "2023-11-11"),
        ("11Nov2023", "2023-11-11"),
        ("1 Jan 99 old", "1999-01-01 old"),
        // Month name, day, year
        ("November 11 2022", "2022-11-11"),
        ("Report Nov 22 2022.pdf", "Report 2022-11-22.pdf"),
        ("June 5 2023", "2023-06-05"),
        ("July 12, 2022.txt", "2022-07-12.txt"),
        ("Sept 3 2021", "2021-09-03"),
        // Compact
        ("IMG_20231105_123456.jpg", "IMG_2023-11-05_123456.jpg"),
        // A non-date word doesn't hide a real date later on
        ("Week 12 2023 - June 5 2023", "Week 12 2023 - 2023-06-05"),
    ];
    for (name, expected) in cases {
        assert_eq!(
            new_name_for(name, false).as_deref(),
            Some(expected),
            "renaming {name:?}"
        );
    }
}

#[test]
fn leaves_non_dates_alone() {
    for name in [
        "2023-11-05 already fine.txt",
        "invoice 12345678",
        "Week 12 2023",
        "31-02-2023",
        "version 1.2.3",
        "notes.txt",
        "Mars 22 2020",
    ] {
        assert_eq!(
            new_name_for(name, false),
            None,
            "{name:?} should be left alone"
        );
    }
}

#[test]
fn month_first_reads_us_dates() {
    assert_eq!(extract_date("05-11-2023", false).unwrap().iso, "2023-11-05");
    assert_eq!(extract_date("05-11-2023", true).unwrap().iso, "2023-05-11");
}

#[test]
fn only_the_first_date_changes() {
    assert_eq!(
        new_name_for("5 Nov 2023 and 6 Dec 2023", false).as_deref(),
        Some("2023-11-05 and 6 Dec 2023")
    );
}

#[test]
fn recursive_plan_lists_inner_items_before_their_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("Trip 5 June 2023");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("photo 20230605.jpg"), "x").unwrap();

    let names: Vec<_> = plan(tmp.path(), true, false)
        .unwrap()
        .iter()
        .map(|c| c.from.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["photo 20230605.jpg", "Trip 5 June 2023"]);

    // Not recursive: only the folder itself.
    assert_eq!(plan(tmp.path(), false, false).unwrap().len(), 1);
}

#[test]
fn clashes_are_flagged_and_numbered_or_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("2023-06-05.txt"), "already there").unwrap();
    fs::write(tmp.path().join("5 June 2023.txt"), "x").unwrap();

    let changes = plan(tmp.path(), false, false).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(changes[0].clash);

    assert!(matches!(
        apply(&changes, OnClash::Skip)[0],
        Outcome::Skipped { .. }
    ));
    assert!(tmp.path().join("5 June 2023.txt").exists());

    assert!(apply(&changes, OnClash::Number)[0].is_renamed());
    assert!(tmp.path().join("2023-06-05_2.txt").exists());
    assert_eq!(
        fs::read_to_string(tmp.path().join("2023-06-05.txt")).unwrap(),
        "already there"
    );
}

#[test]
fn undo_puts_everything_back() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("Trip 5 June 2023");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("photo 20230605.jpg"), "x").unwrap();
    fs::write(tmp.path().join("Report Nov 22 2022.pdf"), "x").unwrap();

    let outcomes = apply(&plan(tmp.path(), true, false).unwrap(), OnClash::Number);
    assert!(outcomes.iter().all(Outcome::is_renamed));
    assert!(tmp
        .path()
        .join("Trip 2023-06-05/photo 2023-06-05.jpg")
        .exists());

    let journal = Journal::from_outcomes(tmp.path(), &outcomes).unwrap();
    let saved = tmp.path().join("state/undo.json");
    journal.save(&saved).unwrap();
    let undone = Journal::load(&saved).unwrap().undo();
    assert!(undone.iter().all(Outcome::is_renamed));
    assert!(folder.join("photo 20230605.jpg").exists());
    assert!(tmp.path().join("Report Nov 22 2022.pdf").exists());
}
