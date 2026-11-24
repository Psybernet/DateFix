//! datefix: the command-line version of DateFix.

use clap::Parser;
use datefix_core::{
    data_dir_next_to_exe, plan, rename_one, Journal, OnClash, Outcome, DATA_FOLDER, UNDO_FILE,
};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Rename files and folders so the dates in their names read YYYY-MM-DD.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Folder to scan (asked for if left out)
    directory: Option<PathBuf>,
    /// Also fix names inside subfolders
    #[arg(short, long)]
    recursive: bool,
    /// Read 05-11-2023 as 11 May (US style) instead of 5 November
    #[arg(short, long)]
    month_first: bool,
    /// Only show what would change
    #[arg(short = 'n', long)]
    dry_run: bool,
    /// Don't ask: rename, adding _2, _3… when a name is taken
    #[arg(short, long)]
    yes: bool,
    /// Put back the names changed by the last rename
    #[arg(long)]
    undo: bool,
}

fn ask(prompt: &str) -> String {
    print!("{prompt}");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();
    line.trim().to_string()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Undo history lives in "DateFix data" next to datefix.exe, or in the
/// temp folder if that folder can't be written to.
fn journal_path() -> PathBuf {
    data_dir_next_to_exe()
        .unwrap_or_else(|| std::env::temp_dir().join(DATA_FOLDER))
        .join(UNDO_FILE)
}

fn report(outcomes: &[Outcome]) {
    for outcome in outcomes {
        match outcome {
            Outcome::Renamed { from, to } => {
                println!("Renamed: {} -> {}", file_name(from), file_name(to))
            }
            Outcome::Skipped { from, reason } => println!("Kept: {} ({reason})", file_name(from)),
            Outcome::Failed { from, error } => {
                println!("Error renaming {}: {error}", from.display())
            }
        }
    }
}

fn undo() -> ExitCode {
    let Some(journal) = Journal::load(&journal_path()) else {
        println!("Nothing to undo.");
        return ExitCode::SUCCESS;
    };
    let outcomes = journal.undo();
    report(&outcomes);
    let _ = std::fs::remove_file(journal_path());
    let done = outcomes.iter().filter(|o| o.is_renamed()).count();
    println!("\nPut back {done} of {} names.", outcomes.len());
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.undo {
        return undo();
    }

    let directory = args.directory.unwrap_or_else(|| {
        PathBuf::from(ask("Enter the directory path to scan: ").trim_matches('"'))
    });
    if !directory.is_dir() {
        eprintln!("Invalid directory path!");
        return ExitCode::FAILURE;
    }

    let changes = match plan(&directory, args.recursive, args.month_first) {
        Ok(changes) => changes,
        Err(e) => {
            eprintln!("Couldn't read {}: {e}", directory.display());
            return ExitCode::FAILURE;
        }
    };
    if changes.is_empty() {
        println!("No files or directories found that need date format changes.");
        return ExitCode::SUCCESS;
    }

    println!("\nProposed changes:");
    for change in &changes {
        let shown = change.from.strip_prefix(&directory).unwrap_or(&change.from);
        let note = if change.clash { "  (name taken)" } else { "" };
        println!("'{}' -> '{}'{note}", shown.display(), file_name(&change.to));
    }

    if args.dry_run {
        println!("\nDry run: no changes were made.");
        return ExitCode::SUCCESS;
    }
    if !args.yes && !ask("\nDo you want to apply these changes? (y/n): ").eq_ignore_ascii_case("y")
    {
        println!("\nNo changes were made.");
        return ExitCode::SUCCESS;
    }

    let mut outcomes = Vec::new();
    for change in &changes {
        let on_clash = if change.to.exists() && !args.yes {
            println!("\nTarget already exists: {}", file_name(&change.to));
            loop {
                match ask("Would you like to:\n1. Keep original name\n2. Add version number\nChoose (1 or 2): ").as_str() {
                    "1" => break OnClash::Skip,
                    "2" => break OnClash::Number,
                    _ => println!("Invalid choice. Please enter 1 or 2."),
                }
            }
        } else {
            OnClash::Number
        };
        let outcome = rename_one(&change.from, &change.to, on_clash);
        report(std::slice::from_ref(&outcome));
        outcomes.push(outcome);
    }

    if let Some(journal) = Journal::from_outcomes(&directory, &outcomes) {
        if journal.save(&journal_path()).is_ok() {
            println!("\nChanged your mind? Run: datefix --undo");
        }
    }
    let done = outcomes.iter().filter(|o| o.is_renamed()).count();
    println!(
        "\nCompleted! Successfully renamed {done} out of {} items.",
        changes.len()
    );
    ExitCode::SUCCESS
}
