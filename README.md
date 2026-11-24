# DateFix

[![Tests](https://github.com/Psybernet/DateFix/actions/workflows/tests.yml/badge.svg)](https://github.com/Psybernet/DateFix/actions/workflows/tests.yml)

Messy dates in file names make files hard to sort. DateFix finds the date in each file or folder name and rewrites it as **YYYY-MM-DD**, so everything lines up in date order.

| Before | After |
|---|---|
| `11 November 2022 notes.txt` | `2022-11-11 notes.txt` |
| `Report Nov 22 2022.pdf` | `Report 2022-11-22.pdf` |
| `scan 05.11.2023.jpg` | `scan 2023-11-05.jpg` |
| `IMG_20231105_123456.jpg` | `IMG_2023-11-05_123456.jpg` |

It always shows you the changes first and only renames after you say yes.

## How to use it

You need [Python](https://www.python.org/downloads/) 3.10 or newer. Nothing else to install.

1. Download [`date_fix.py`](date_fix.py).
2. Run it:

   ```bash
   python date_fix.py
   ```

3. Type the folder you want to tidy and press Enter.
4. Check the list of changes, then type `y` to rename.

If a new name is already taken, it asks whether to skip that file or add a number (`2023-06-05_2.txt`).

## Options

You can give the folder and options on the command line instead:

```bash
python date_fix.py "D:\Scans" -r -n
```

| Option | What it does |
|---|---|
| `-r` | Include subfolders |
| `-n` | Dry run: show the changes without renaming anything |
| `-y` | Rename without asking |
| `-m` | Read `05-11-2023` as 11 May (US style) instead of 5 November |

## Dates it recognises

`2023-11-05`, `2023_11_05`, `05.11.2023`, `11 November 2022`, `3rd July 2021`, `11Nov23`, `Nov 22, 2022`, `Sept 3 2021` and `20231105`.

Only the first date in a name is changed. Anything that isn't a real date, like `31-02-2023` or `invoice 12345678`, is left alone.

## For developers

```bash
pip install pytest
python -m pytest
```

The tests also run automatically on Windows and Linux every time a change is pushed.

## License

[MIT](LICENSE)
