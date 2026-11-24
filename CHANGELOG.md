# Changelog

## 2.1.0 (2026-11-24)

- Everything DateFix saves (the undo history and the window's browser data) now lives in a **`DateFix data` folder next to the program**, for both the installed and portable versions. Nothing goes in hidden AppData folders, and uninstalling removes the folder too.
- New **portable version**: `DateFix-v2.1.0-portable.exe` runs without installing.
- The command-line tool keeps its undo history in the same `DateFix data` folder next to `datefix.exe`.
- The installer installs for the current user only (in `%LOCALAPPDATA%\DateFix`), so it never needs administrator rights.
- The MSI download is gone: it installs into Program Files, where DateFix can't keep its data next to itself.

## 2.0.0 (2026-11-24)

- Rebuilt as a desktop app (Rust + Tauri) with a glass interface, drag and drop, a live preview, tick boxes to skip items, a filter, and undo that survives a restart.
- Command-line tool rewritten in Rust with `--undo`.
- Fixes from the 2024 Python version: "June 5 2023", "July 12 2022" and "Sept" dates are now recognised, and a non-date word no longer hides a real date later in the name.
- Automated tests on Windows and Linux, and automatic release builds.
