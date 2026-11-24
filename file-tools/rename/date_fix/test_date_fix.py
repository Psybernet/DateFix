"""Tests for date_fix. Run with:  python -m pytest"""

import os

import pytest

from date_fix import extract_date, main, new_name_for, scan_directory


@pytest.mark.parametrize("name, expected", [
    # Numeric, year first
    ("2023-11-5 notes", "2023-11-05 notes"),
    ("2023_11_05 photo", "2023-11-05 photo"),
    ("2023.11.05 scan", "2023-11-05 scan"),
    # Numeric, day first
    ("05-11-2023 report", "2023-11-05 report"),
    ("5.11.2023.pdf", "2023-11-05.pdf"),
    # Day, month name, year
    ("11 November 2022 notes.txt", "2022-11-11 notes.txt"),
    ("5 June 2023", "2023-06-05"),
    ("3rd July 2021 trip", "2021-07-03 trip"),
    ("11Nov23", "2023-11-11"),
    ("11Nov2023", "2023-11-11"),
    ("1 Jan 99 old", "1999-01-01 old"),
    # Month name, day, year
    ("November 11 2022", "2022-11-11"),
    ("Report Nov 22 2022.pdf", "Report 2022-11-22.pdf"),
    ("June 5 2023", "2023-06-05"),          # 4-letter month: broken in v1
    ("July 12, 2022.txt", "2022-07-12.txt"),
    ("Sept 3 2021", "2021-09-03"),          # "Sept": broken in v1
    # Compact
    ("IMG_20231105_123456.jpg", "IMG_2023-11-05_123456.jpg"),
    # Skips a non-date word and finds the real date later in the name
    ("Week 12 2023 - June 5 2023", "Week 12 2023 - 2023-06-05"),
])
def test_renames(name, expected):
    assert new_name_for(name) == expected


@pytest.mark.parametrize("name", [
    "2023-11-05 already fine.txt",
    "invoice 12345678",        # 8 digits but not a 19xx/20xx date
    "Week 12 2023",            # "Week" isn't a month
    "31-02-2023",              # no 31st of February
    "version 1.2.3",
    "notes.txt",
])
def test_leaves_alone(name):
    assert new_name_for(name) is None


def test_month_first():
    assert extract_date("05-11-2023")[0] == "2023-11-05"
    assert extract_date("05-11-2023", month_first=True)[0] == "2023-05-11"


def test_only_first_date_changes():
    assert new_name_for("5 Nov 2023 and 6 Dec 2023") == "2023-11-05 and 6 Dec 2023"


def test_recursive_renames_inner_items_before_folders(tmp_path):
    folder = tmp_path / "Trip 5 June 2023"
    folder.mkdir()
    (folder / "photo 20230605.jpg").write_text("x")

    changes = scan_directory(str(tmp_path), recursive=True)
    olds = [os.path.basename(old) for old, _ in changes]
    assert olds == ["photo 20230605.jpg", "Trip 5 June 2023"]


def test_dry_run_changes_nothing(tmp_path):
    (tmp_path / "5 June 2023.txt").write_text("x")
    main([str(tmp_path), "--dry-run"])
    assert os.listdir(tmp_path) == ["5 June 2023.txt"]


def test_yes_renames_and_versions_clashes(tmp_path):
    (tmp_path / "2023-06-05.txt").write_text("already there")
    (tmp_path / "5 June 2023.txt").write_text("x")
    main([str(tmp_path), "--yes"])
    assert sorted(os.listdir(tmp_path)) == ["2023-06-05.txt", "2023-06-05_2.txt"]
