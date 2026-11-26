# DateFix: notes for Claude

DateFix renames files and folders so the dates in their names read YYYY-MM-DD.
It's a Windows desktop app (Tauri 2 + Rust) with a glass interface, plus a
command-line tool. Owner: Martin Heine (GitHub: Psybernet). It's a public
portfolio project: Martin is looking for DevOps, cybersecurity, sysadmin and
AI-assisted developer roles, so clean code, tests and CI matter as much as features.

## Layout

```text
crates/datefix-core   Library: date parsing (parse.rs), planning and renaming
                      (files.rs), undo journal (undo.rs), data folder (storage.rs)
crates/datefix-cli    Command-line tool, binary name datefix-cli
src-tauri             Desktop app backend (main.rs), tauri.conf.json,
                      capabilities/, icons/, windows/hooks.nsh (NSIS hooks)
ui                    Interface: index.html, styles.css, main.js. Plain
                      HTML/CSS/JS, no framework, no bundler (withGlobalTauri)
docs/screenshot.png   README screenshot
```

The app and the CLI must both go through `datefix-core`, so they always
understand the same dates. Never duplicate date logic in the UI.

## Commands

```bash
cargo test -p datefix-core -p datefix-cli    # tests
cargo fmt --all                              # CI fails on unformatted code
cargo clippy -p datefix-core -p datefix-cli -p datefix-app -- -D warnings
npx @tauri-apps/cli@2 dev                    # run the app (Windows)
cargo run -p datefix-cli -- "D:\Scans" -n    # try the CLI as a dry run
```

Opening `ui/index.html` in a browser shows the interface with demo data (the
`demoBackend()` at the bottom of main.js), which is handy for UI work and
screenshots without building the app.

## Decisions Martin has made (keep these)

- **Everything stays next to the program.** Undo history (`last-rename.json`)
  and WebView2 data go in a `DateFix data` folder next to the exe, for the
  installed version, the portable version and the CLI. Fall back to app data
  only if that folder can't be written. Don't add files in AppData or the
  registry. That's why the window is created in code (`"create": false` in
  tauri.conf.json) with `.data_directory(...)`.
- **Clean uninstalls.** The NSIS installer is per-user (`installMode:
  currentUser`, installs to `%LOCALAPPDATA%\DateFix`), and
  `windows/hooks.nsh` deletes `DateFix data` on uninstall. No MSI: it installs
  into Program Files, which isn't writable, so it can't keep data next to itself.
- **Each release has three downloads:** `DateFix-vX.Y.Z-setup.exe`,
  `DateFix-vX.Y.Z-portable.exe` (the app window, no install) and
  `datefix-cli-vX.Y.Z.exe`.
- **It never overwrites a file.** When a new name is taken, it either adds
  `_2`, `_3`… or skips the file, as the user chooses.
- **Day-first dates by default** (Martin is in Australia); US order is an option.
- Style: dark glass UI (Windows Mica/Acrylic via window-vibrancy, CSS
  backdrop blur), purple-to-cyan accent gradient. Keep new UI consistent.

## Releasing

1. Bump the version in **both** `Cargo.toml` (workspace) and
   `src-tauri/tauri.conf.json`, and add an entry to `CHANGELOG.md`.
2. Push to main and wait for the **Tests** workflow to pass.
3. Martin publishes the release on GitHub (**Releases → Draft a new release**,
   new tag `vX.Y.Z` on main). Pushing tags from Claude's environment has been
   blocked, so give him these steps rather than tagging yourself.
4. `.github/workflows/release.yml` checks that the tag matches both versions,
   runs the tests, builds the installer, portable exe and CLI, and attaches
   them to the release. It takes about 10–15 minutes.

## Lessons learned (don't repeat these)

- **Windows file names ignore case.** The CLI was once named `datefix.exe`,
  which overwrote `DateFix.exe` in `target/release`. Binary names must differ
  by more than case. The release workflow now checks that the two files differ.
- **GitHub runners drop old toolchains.** Python 3.9 vanished from Ubuntu
  26.04 runners. Keep actions on current major versions (checkout@v5,
  setup-node@v5).
- Windows-only code (`#[cfg(windows)]`, window-vibrancy) can't be compiled on
  Linux. The Windows job in tests.yml runs clippy on `datefix-app` to catch it.
- `fs::rename` on Windows replaces an existing file, so always check
  `exists()` first (see `rename_one`).

## Next up: features from Martin's V6 Python app

Before the Rust rewrite, Martin had a PyQt version (V6, June 2026) that never
reached GitHub. It has things 2.x is missing, so these come first:

1. ~~Work out the day/month order when it's obvious~~ (done in 2.1.2, 4-digit
   years only: with a 2-digit year the order is never swapped, because times
   like `10-15-30` would become dates).
2. ~~2-digit years in numeric dates~~ (done in 2.1.2: `-` or `.` only, and
   with `.` the day and month need two digits each, so `1.2.23` isn't a date).
3. **"Include folders" option** (on by default), so you can rename only files.
   It needs an option in the core `plan()`, the UI and a CLI flag.
4. **Scan in the background with progress and Cancel.** Tauri runs sync
   commands on the main thread, so a large recursive scan freezes the window.
   Make `scan` async or run it on a thread, and send progress events.

Martin's real test names (from his Test/Template folders) are in
`martins_test_names` in `crates/datefix-core/tests/core.rs` and must keep passing.

How the parser picks a date (parse.rs): every shape is searched one character
at a time, so a failed match can't hide a real date. Of all real dates found,
one with a 4-digit year beats a 2-digit one, then the earliest in the name
wins. Month names must be whole words (`Mar`, not `Mars`), and month-name
dates need a separator between day and year, so `June 2024` isn't 20 June '24.

## Ideas not done yet

- Signing the installer, so Windows doesn't show "unknown publisher"
- Auto-update (Tauri updater plugin)
- Remembering the last folder and options between launches (save them in `DateFix data`)
- Light theme and following the Windows theme
- Undo history for more than the last batch
- winget or Scoop package for installing from the command line

## Commits

End commit messages with:

```
Co-Authored-By: Claude <noreply@anthropic.com>
```
