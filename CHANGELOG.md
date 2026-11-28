# Changelog

## 2.2.0 (unreleased)

Includes the date fixes planned for 2.1.2, which was never released.

**A new interface**

- A plainer, more functional look: dark by default with a light mode, purple highlights, and no see-through glass effect. Your theme and options are remembered.
- A compact table that shows about twice as many rows, with sortable columns (current name, new name, date, folder, status). It only draws the rows on screen, so it stays quick with tens of thousands of files.
- **Status, most urgent first.** Red for items that can't be renamed right now (in use by another program, or no permission), amber for things to check (name taken, two items would get the same name, path over 260 characters, or **Day or month?** when the date could be read either way). Tabs filter to All, Can't rename, Check and Unticked.
- **Several folders at once.** Add folders from anywhere; with more than one, a **Folders** view shows a tree with counts and tick boxes for each folder. Switch between **List** and **Folders** at any time.
- **Scanning runs in the background** with progress and a **Cancel** button, so the window never freezes. Renaming shows progress too.
- **History page:** every past rename, a check of each file before undoing, and a choice of what to put back. Forget a rename you no longer need.
- **After a crash**, DateFix offers to undo or finish the rename that was interrupted.
- New option: **Rename folders too** (on by default), and `--files-only` on the command line.
- Scanning is faster: name clashes are found from the folder listing, and the in-use check runs on several threads.

**Undo history that survives crashes**

- Every rename is kept in a **rename history** in `DateFix data\history`, not just the last one. The newest 30 batches are kept.
- **Crash-safe.** The whole plan is saved to disk before anything is renamed, and each rename is written down as it happens. If DateFix, Windows or the power stops part way, the history knows exactly what was renamed. If the history can't be saved, nothing is renamed.
- **Files are recognised by a fingerprint** (size and a SHA-256 of the start and end of the file). Undo finds a file that was renamed again since, and it won't put the old name on a file whose contents have changed unless you choose to.
- **Undo never loses track.** Items that can't be put back (moved, deleted, or the old name is taken) stay in the history, so you can fix the problem and undo again. Before, a partly failed undo forgot the rest.
- The old `last-rename.json` is moved into the new history automatically.
- Command line: `--history` lists past renames, `--undo [ID]` undoes the newest or a chosen one, and `--resume [ID]` finishes an interrupted rename. It exits with an error code when something fails.

**Dates**

- New: when only one day/month order makes a real date, DateFix uses it. `3.13.2024` becomes 13 March 2024 instead of being skipped.
- New: 2-digit years in numeric dates, such as `05-11-23` and `05.11.23`. With dots, the day and month need two digits each, so version numbers like `1.2.23` are left alone.
- Fixed: a month and year with no day, like `Report June 2024`, was read as 20 June 2024. It's now left alone.
- Fixed: a number followed by a word could hide a real date, so `Week 3 notes 14 March 2024` became the wrong date.
- Fixed: the new date could land in the wrong place when the same text appeared earlier in the name (`105-11-2023 then 05-11-2023`).
- Fixed: version numbers like `v1.2.2023` and `10.1.2.2023` are no longer read as dates.
- Changed: DateFix now fixes the first date in the name, as the README always said. Before, some date styles were tried before others, wherever they were in the name. A date with a 4-digit year still beats a 2-digit one.

## 2.1.1 (2026-11-25)

- Fixed: the portable download was the command-line tool instead of the app. On Windows, `datefix.exe` and `DateFix.exe` count as the same file name, so the command-line build overwrote the app. The command-line tool is now called `datefix-cli.exe`.

## 2.1.0 (2026-11-24)

- Everything DateFix saves (the undo history and the window's browser data) now lives in a **`DateFix data` folder next to the program**, for both the installed and portable versions. Nothing goes in hidden AppData folders, and uninstalling removes the folder too.
- New **portable version**: `DateFix-v2.1.0-portable.exe` runs without installing.
- The command-line tool keeps its undo history in the same `DateFix data` folder next to the command-line tool.
- The installer installs for the current user only (in `%LOCALAPPDATA%\DateFix`), so it never needs administrator rights.
- The MSI download is gone: it installs into Program Files, where DateFix can't keep its data next to itself.

## 2.0.0 (2026-11-24)

- Rebuilt as a desktop app (Rust + Tauri) with a glass interface, drag and drop, a live preview, tick boxes to skip items, a filter, and undo that survives a restart.
- Command-line tool rewritten in Rust with `--undo`.
- Fixes from the 2024 Python version: "June 5 2023", "July 12 2022" and "Sept" dates are now recognised, and a non-date word no longer hides a real date later in the name.
- Automated tests on Windows and Linux, and automatic release builds.
