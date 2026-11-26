# Changelog

## 2.1.2 (unreleased)

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
