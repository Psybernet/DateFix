# DateFix: notes for Claude

DateFix renames files and folders so the dates in their names read YYYY-MM-DD.
It's a Windows desktop app (Tauri 2 + Rust) with a glass interface, plus a
command-line tool. Owner: Martin Heine (GitHub: Psybernet). It's a public
portfolio project: Martin is looking for DevOps, cybersecurity, sysadmin and
AI-assisted developer roles, so clean code, tests and CI matter as much as features.

## Layout

```text
crates/datefix-core   Library: date parsing (parse.rs), planning and renaming
                      (files.rs), rename history (history.rs), data folder
                      (storage.rs), old pre-2.2 undo file (undo.rs)
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
screenshots without building the app. Hashes: `#single` (one folder, list),
no hash (three folders, tree), `#history`, `#recover` (crash dialog),
`#demo-empty`, and `-light` / `light` for light mode. Scripts need a page
served over http or file:// in a real browser (headless Edge works:
`msedge --headless=new --window-size=1240,800 --virtual-time-budget=4000
--screenshot=out.png file:///.../ui/index.html#single`).

To drive the real app in tests, run a **debug** build with
`DATEFIX_DEBUG_PORT=9333`: it opens the WebView2 debugging port (Chrome
DevTools protocol). Don't use `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`: it
clashes with Tauri's own arguments and the window fails to open. If the
window fails with "resource in use", an old msedgewebview2.exe still holds
`target\debug\DateFix data\webview`.

## Decisions Martin has made (keep these)

- **Everything stays next to the program.** The rename history (`history\`)
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
- Style (2.2): functional and calm. Dark by default with a light mode
  (`data-theme` on `<html>`), neutral greys, purple accents in a few shades
  (`--p-200`…`--p-900`), amber for "check this", red only for "can't be
  renamed". No glass, gradients or glow. Segoe UI, 30 px rows. Keep new UI
  consistent with `ui/styles.css` tokens. The agreed mockup is
  docs/ui-concepts/concepts-v2.html (open it with #list, #tree, #history,
  #recover or #list-light).

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

## Rename history (history.rs)

Undo must survive crashes, restarts and files being renamed again since.

- One `.jsonl` file per batch in `DateFix data\history`. Lines are only
  appended: `batch` header, a `plan` line per item (with fingerprint), all
  synced to disk **before the first rename**; then `done` per rename,
  `finished` at the end, and `undone` per name put back. A torn last line is
  ignored. No `finished` line means the batch was interrupted.
- If the plan can't be saved, nothing is renamed (`History::run` returns Err).
- Fingerprint = size + SHA-256 of the first and last 64 KiB. Used to find
  renames whose `done` line was lost in a crash (`recover`), files renamed
  again since (`RenamedSince`), and changed contents (`ContentsChanged`,
  never undone unless chosen).
- Folders come after their contents in a batch; `Batch::current` maps
  recorded paths through folders renamed in the same batch.
- Undo writes an `undone` line per item and keeps the batch, so a partly
  failed undo can be retried. Keep the newest 30 finished batches; never
  prune interrupted ones.
- The app shows it on the History page and offers recovery on start
  (`offerRecovery` in main.js runs `history_check` first, so lost renames
  are counted).

## Lessons learned (don't repeat these)

- **Windows file names ignore case.** The CLI was once named `datefix.exe`,
  which overwrote `DateFix.exe` in `target/release`. Binary names must differ
  by more than case. The release workflow now checks that the two files differ.
- **GitHub runners drop old toolchains.** Python 3.9 vanished from Ubuntu
  26.04 runners. Keep actions on current major versions (checkout@v5,
  setup-node@v5).
- Windows-only code (`#[cfg(windows)]`, such as `precheck`) can't be compiled on
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
3. ~~"Include folders" option~~ (done in 2.2: `PlanOptions::include_dirs`,
   "Rename folders too", `--files-only`).
4. ~~Scan in the background with progress and Cancel~~ (done in 2.2: async
   commands on `spawn_blocking`, `scan-progress` / `rename-progress` events,
   `cancel_scan`).

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
- Remembering the last folders between launches (options and theme are
  already remembered, in the WebView's storage inside `DateFix data`)
- Export a rename from the history as CSV (shown in the mockups, not built)
- Following the Windows light/dark setting (light and dark both exist)
- winget or Scoop package for installing from the command line

## Commits

End commit messages with:

```
Co-Authored-By: Claude <noreply@anthropic.com>
```
