# date_fix

Renames files and folders so the dates in their names all read **YYYY-MM-DD**. Files then sort by date in any file manager.

| Before | After |
|---|---|
| `11 November 2022 notes.txt` | `2022-11-11 notes.txt` |
| `Report Nov 22 2022.pdf` | `Report 2022-11-22.pdf` |
| `June 5 2023 trip` | `2023-06-05 trip` |
| `scan 05.11.2023.jpg` | `scan 2023-11-05.jpg` |
| `11Nov23` | `2023-11-11` |
| `IMG_20231105_123456.jpg` | `IMG_2023-11-05_123456.jpg` |

It shows every change and asks before renaming anything.

## Requirements

Python 3.10 or newer. No extra packages.

## Usage

```bash
python date_fix.py                      # asks which folder to scan
python date_fix.py "D:\Scans"           # scan a folder
python date_fix.py "D:\Scans" -r        # include subfolders
python date_fix.py "D:\Scans" -n        # dry run: only show what would change
python date_fix.py "D:\Scans" -y        # apply without asking
```

| Option | What it does |
|---|---|
| `-r`, `--recursive` | Also fixes names inside subfolders. Items inside a folder are renamed before the folder itself. |
| `-m`, `--month-first` | Reads `05-11-2023` as 11 May (US style). By default it's read as 5 November. |
| `-n`, `--dry-run` | Lists the changes without renaming anything. |
| `-y`, `--yes` | Applies the changes without asking, and adds `_2`, `_3`… when a new name is already taken. |

If a new name is already taken, it asks whether to keep the original name or add a version number (`2023-06-05_2.txt`).

## Date formats it understands

- **Numbers, year first:** `2023-11-05`, `2023_11_05`, `2023.11.05`, `2023 11 05`
- **Numbers, day first:** `05-11-2023`, `5.11.2023`, `05_11_2023` (or month first with `-m`)
- **Day, month, year:** `11 November 2022`, `3rd July 2021`, `11 Nov 23`, `11Nov2023`
- **Month, day, year:** `November 11 2022`, `Nov 22, 2022`, `Sept 3 2021`
- **Compact:** `20231105` (years 1900–2099)

Months can be full names or three-letter short forms, plus `Sept`. Two-digit years 00–49 become 20xx, and 50–99 become 19xx. Only the first date in a name is changed, and anything that isn't a real date, such as 31-02-2023 or `invoice 12345678`, is left alone.

## Tests

```bash
pip install pytest
python -m pytest
```

The tests also run on GitHub, on Windows and Linux, every time a change is pushed.
