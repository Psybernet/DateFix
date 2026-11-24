#!/usr/bin/env python3
"""DateFix: rename files and folders so the dates in their names read YYYY-MM-DD.

Examples of names it fixes:
    "11 November 2022 notes.txt"  -> "2022-11-11 notes.txt"
    "Report Nov 22 2022.pdf"      -> "Report 2022-11-22.pdf"
    "scan 05.11.2023.jpg"         -> "scan 2023-11-05.jpg"
    "IMG_20231105_123456.jpg"     -> "IMG_2023-11-05_123456.jpg"

Run it with no arguments to be asked for a folder, or see --help.
"""

import argparse
import calendar
import os
import re
import sys
from datetime import datetime

__version__ = "2.0.0"

# Month names to numbers: full names, 3-letter abbreviations and "sept".
MONTHS = {}
for _number, _name in enumerate(calendar.month_name[1:], 1):
    MONTHS[_name.lower()] = _number
    MONTHS[_name.lower()[:3]] = _number
MONTHS["sept"] = 9

_SEP = r"[-._ ]"          # separators allowed between numeric date parts
_GAP = r"[-._ ,]*"        # looser gap allowed around a month name
_ORD = r"(?:st|nd|rd|th)?"  # optional ordinal suffix: 1st, 2nd, 3rd, 4th

# Tried in order; the first pattern with a valid date wins.
PATTERNS = [
    # 2023-11-05, 2023_11_05, 2023.11.05, 2023 11 05
    re.compile(rf"(?<!\d)(?P<y>\d{{4}})(?P<s>{_SEP})(?P<m>\d{{1,2}})(?P=s)(?P<d>\d{{1,2}})(?!\d)"),
    # 05-11-2023, 05_11_2023, 05.11.2023 (day first unless --month-first)
    re.compile(rf"(?<!\d)(?P<a>\d{{1,2}})(?P<s>{_SEP})(?P<b>\d{{1,2}})(?P=s)(?P<y>\d{{4}})(?!\d)"),
    # 11 November 2022, 11Nov23, 3rd June 2021
    re.compile(rf"(?<!\d)(?P<d>\d{{1,2}}){_ORD}{_GAP}(?P<mon>[A-Za-z]{{3,9}}){_GAP}(?P<y>\d{{4}}|\d{{2}})(?!\d)"),
    # November 11 2022, Nov 22, 2022, June 5 2023, Nov2223
    re.compile(rf"(?<![A-Za-z])(?P<mon>[A-Za-z]{{3,9}}){_GAP}(?P<d>\d{{1,2}}){_ORD}{_GAP}(?P<y>\d{{4}}|\d{{2}})(?!\d)"),
    # 20231105 (years 1900-2099 only, to avoid renaming other 8-digit numbers)
    re.compile(r"(?<!\d)(?P<y>(?:19|20)\d{2})(?P<m>\d{2})(?P<d>\d{2})(?!\d)"),
]


def _full_year(year):
    """Turn a 2-digit year into 4 digits: 00-49 -> 20xx, 50-99 -> 19xx."""
    if len(year) == 2:
        return 2000 + int(year) if int(year) < 50 else 1900 + int(year)
    return int(year)


def _date_from_match(match, month_first):
    """Return a datetime for a regex match, or None if it isn't a real date."""
    parts = match.groupdict()
    if parts.get("mon") is not None:
        month = MONTHS.get(parts["mon"].lower())
        if month is None:
            return None
    elif parts.get("a") is not None:
        day, month = (parts["b"], parts["a"]) if month_first else (parts["a"], parts["b"])
        parts["d"] = day
        month = int(month)
    else:
        month = int(parts["m"])
    try:
        return datetime(_full_year(parts["y"]), month, int(parts["d"]))
    except ValueError:
        return None


def extract_date(name, month_first=False):
    """Find the first date in a name.

    Returns (date as "YYYY-MM-DD", the text that matched), or (None, None).
    """
    for pattern in PATTERNS:
        for match in pattern.finditer(name):
            date = _date_from_match(match, month_first)
            if date is not None:
                return date.strftime("%Y-%m-%d"), match.group()
    return None, None


def new_name_for(name, month_first=False):
    """Return the fixed name, or None if the name needs no change."""
    new_date, old_text = extract_date(name, month_first)
    if new_date is None or old_text == new_date:
        return None
    return name.replace(old_text, new_date, 1)


def scan_directory(directory, recursive=False, month_first=False):
    """List (old path, new path) for everything whose name needs fixing.

    In recursive mode the deepest items come first, so renaming a folder
    never breaks the paths of the items inside it.
    """
    changes = []
    if recursive:
        for root, dirs, files in os.walk(directory, topdown=False):
            for item in sorted(files) + sorted(dirs):
                new = new_name_for(item, month_first)
                if new:
                    changes.append((os.path.join(root, item), os.path.join(root, new)))
    else:
        for item in sorted(os.listdir(directory)):
            new = new_name_for(item, month_first)
            if new:
                changes.append((os.path.join(directory, item), os.path.join(directory, new)))
    return changes


def get_next_version_name(path):
    """Add _2, _3... before the extension until the name is free."""
    directory, filename = os.path.split(path)
    name, ext = os.path.splitext(filename)
    version = 2
    while os.path.exists(path):
        path = os.path.join(directory, f"{name}_{version}{ext}")
        version += 1
    return path


def handle_rename(old_path, new_path, assume_yes=False):
    """Rename one item, asking what to do if the new name is taken."""
    if os.path.exists(new_path):
        print(f"\nTarget already exists: {os.path.basename(new_path)}")
        if assume_yes:
            choice = "2"
        else:
            while True:
                choice = input("Would you like to:\n"
                               "1. Keep original name\n"
                               "2. Add version number\n"
                               "Choose (1 or 2): ").strip()
                if choice in ("1", "2"):
                    break
                print("Invalid choice. Please enter 1 or 2.")
        if choice == "1":
            print(f"Keeping original name: {os.path.basename(old_path)}")
            return False
        new_path = get_next_version_name(new_path)
    try:
        os.rename(old_path, new_path)
    except OSError as error:
        print(f"Error renaming {old_path}: {error}")
        return False
    print(f"Renamed: {os.path.basename(old_path)} -> {os.path.basename(new_path)}")
    return True


def parse_args(argv=None):
    parser = argparse.ArgumentParser(
        description="Rename files and folders so the dates in their names read YYYY-MM-DD.")
    parser.add_argument("directory", nargs="?",
                        help="folder to scan (asked for if left out)")
    parser.add_argument("-r", "--recursive", action="store_true",
                        help="also fix names inside subfolders")
    parser.add_argument("-m", "--month-first", action="store_true",
                        help="read 05-11-2023 as May 11 (US style) instead of 5 November")
    parser.add_argument("-n", "--dry-run", action="store_true",
                        help="only show what would change")
    parser.add_argument("-y", "--yes", action="store_true",
                        help="don't ask: apply the changes and add version numbers on clashes")
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)
    directory = args.directory or input("Enter the directory path to scan: ").strip().strip('"')

    if not os.path.isdir(directory):
        print("Invalid directory path!")
        return 1

    changes = scan_directory(directory, args.recursive, args.month_first)
    if not changes:
        print("No files or directories found that need date format changes.")
        return 0

    print("\nProposed changes:")
    for old_path, new_path in changes:
        shown = os.path.relpath(old_path, directory)
        print(f"'{shown}' -> '{os.path.basename(new_path)}'")

    if args.dry_run:
        print("\nDry run: no changes were made.")
        return 0

    if not args.yes:
        if input("\nDo you want to apply these changes? (y/n): ").strip().lower() != "y":
            print("\nNo changes were made.")
            return 0

    renamed = sum(handle_rename(old, new, args.yes) for old, new in changes)
    print(f"\nCompleted! Successfully renamed {renamed} out of {len(changes)} items.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
