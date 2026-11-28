# <img src="ui/icon.png" width="32" height="32" alt=""> DateFix

[![Tests](https://github.com/Psybernet/DateFix/actions/workflows/tests.yml/badge.svg)](https://github.com/Psybernet/DateFix/actions/workflows/tests.yml)
[![Latest release](https://img.shields.io/github/v/release/Psybernet/DateFix?style=flat-square)](https://github.com/Psybernet/DateFix/releases/latest)
[![Built with Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square&logo=tauri&logoColor=white)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)

Messy dates in file names make files hard to sort. DateFix finds the date in each file or folder name and rewrites it as **YYYY-MM-DD**, so everything lines up in date order.

![DateFix in dark mode, showing 594 names to fix with the ones that need a look sorted to the top](docs/screenshot.png)

| Before | After |
|---|---|
| `11 November 2022 notes.txt` | `2022-11-11 notes.txt` |
| `Report Nov 22 2022.pdf` | `Report 2022-11-22.pdf` |
| `scan 05.11.2023.jpg` | `scan 2023-11-05.jpg` |
| `IMG_20231105_123456.jpg` | `IMG_2023-11-05_123456.jpg` |

## Download

Get **`DateFix-vX.Y.Z-setup.exe`** from the [latest release](https://github.com/Psybernet/DateFix/releases/latest) and run it, or use **`DateFix-vX.Y.Z-portable.exe`** to run it without installing. It works on Windows 10 and 11.

Everything DateFix saves, such as your undo history, goes in a **`DateFix data`** folder next to the program, never in hidden folders elsewhere. Uninstalling removes it too. For the portable version, just delete the `.exe` and its `DateFix data` folder.

The installer isn't code-signed, so Windows may say it's from an unknown publisher. Click **More info → Run anyway**.

## How to use it

1. **Drop folders** onto the window, or click **Add folder** (<kbd>Ctrl</kbd>+<kbd>O</kbd>). You can add several folders, even from different drives.
2. **Check the preview.** Each row shows the old name, the new name and the date it found. The ones that need a look come first:
   - **Red:** it can't be renamed right now, for example because another program has the file open. These are left unticked.
   - **Amber:** worth a check. The new name is taken, two items would get the same name, or the date could be read either way (**Day or month?** shows both readings when you point at it).
   - Use the tabs (**All**, **Can't rename**, **Check**, **Unticked**) and the filter box to find things, and untick anything you want to leave alone.
3. Click **Rename**.

With one folder the preview is a list. With several, it shows a folder tree so you can work through each folder or leave a whole folder out. You can switch between **List** and **Folders** at any time. Large folders scan in the background with a **Cancel** button, and the list stays quick with tens of thousands of files.

DateFix opens in dark mode; the **Light** button in the title bar switches to light mode, and it remembers your choice and options.

**Undo and history.** Click **Undo last rename** (<kbd>Ctrl</kbd>+<kbd>Z</kbd>) or **History** (<kbd>Ctrl</kbd>+<kbd>H</kbd>) to see your past renames. Before undoing, DateFix checks every file: it shows which still match, which you've renamed again since (found by their fingerprint), and which have changed or gone missing, and you choose what to put back. If DateFix was stopped part way through a rename, by a crash or a power cut, it tells you next time it starts and offers to undo it or finish it.

DateFix keeps a **rename history** of your last 30 renames in the `DateFix data` folder next to the program. It saves the plan before renaming anything and writes down each rename as it happens, so even a crash or power cut part way through can be undone. Each file is recognised by a fingerprint, so undo still finds a file you've renamed again since, and it won't touch a file whose contents have changed unless you choose to.

**Options**
- **Include subfolders** also fixes names inside folders within the folder.
- **Rename folders too** (on by default). Turn it off to rename only files.
- **Read 05-11-2023 as** 5 November (day first, the default) or 11 May (US order). If only one order makes a real date, like `3.13.2024`, that one is used.
- **If the new name is taken**, DateFix either adds a number (`2023-06-05_2.txt`) or skips that item. It never overwrites a file.

## Dates it recognises

| Style | Examples |
|---|---|
| Numbers, year first | `2023-11-05`, `2023_11_05`, `2023.11.05` |
| Numbers, day first | `05-11-2023`, `5.11.2023`, `05-11-23` (or month first with US date order) |
| Day, month, year | `11 November 2022`, `3rd July 2021`, `11Nov23` |
| Month, day, year | `November 11 2022`, `Nov 22, 2022`, `Sept 3 2021` |
| Compact | `20231105` (years 1900–2099) |

Only the first date in a name is changed. Anything that isn't a real date, like `31-02-2023`, `invoice 12345678` or `Setup v1.2.2023`, is left alone. If only one day/month order makes a real date, DateFix uses that one, so `3.13.2024` becomes `2024-03-13`.

## Command line

The release also includes **`datefix-cli.exe`**, which does the same from a terminal:

```text
datefix-cli [OPTIONS] [FOLDER]

  -r, --recursive    Also fix names inside subfolders
  -m, --month-first  Read 05-11-2023 as 11 May (US style)
  -f, --files-only   Rename only files, not folders
  -n, --dry-run      Only show what would change
  -y, --yes          Don't ask; add _2, _3… when a name is taken
      --undo [ID]    Put back the names from the last rename (or batch ID)
      --history      List past renames and their IDs
      --resume [ID]  Finish a rename that was interrupted
```

Run it without a folder and it asks for one, shows the changes and asks before renaming.

## How it's built

```text
crates/datefix-core   Rust library: finds dates, plans and applies renames, crash-safe rename history
crates/datefix-cli    Command-line tool built on the core
src-tauri             Desktop app: Tauri 2 backend (scans in the background, history commands)
ui                    The interface: HTML, CSS and JavaScript, no framework or build step
```

The app and the command-line tool share one core library, so they always understand the same dates.

**Automation (GitHub Actions)**
- **[Tests](.github/workflows/tests.yml)** run on every push, on Windows and Linux: formatting, unit and file-system tests, Clippy lints, and a Windows build check of the desktop app.
- **[Release](.github/workflows/release.yml)** runs when a version tag is pushed. It checks that the tag matches the version in the code, runs the tests, builds the installer, the portable app and the command-line tool, and publishes them on the Releases page.

## Developing

You need [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) (only for the Tauri command-line tool) and, on Windows, the WebView2 runtime, which Windows 10 and 11 already include.

```bash
cargo test -p datefix-core -p datefix-cli   # run the tests
npx @tauri-apps/cli@2 dev                   # run the app with live reload
cargo run -p datefix-cli -- "D:\Scans" -n   # try the command-line tool
```

Open `ui/index.html` in a browser to work on the interface with demo data.

See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.

**Releasing:** set the new version in `Cargo.toml` and `src-tauri/tauri.conf.json`, push to main, then publish a release on GitHub with a new tag such as `v2.1.2`. The Release workflow builds and attaches the downloads.

## License

[MIT](LICENSE) © 2024–2026 Martin Heine
