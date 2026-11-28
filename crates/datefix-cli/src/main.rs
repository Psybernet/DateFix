//! datefix-cli: the command-line version of DateFix.

use clap::Parser;
use datefix_core::{
    data_dir_next_to_exe, plan_with, safe_to_undo, Batch, BatchState, History, OnClash, Outcome,
    PlanOptions, DATA_FOLDER,
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
    /// Rename only files, not folders
    #[arg(short, long)]
    files_only: bool,
    /// Only show what would change
    #[arg(short = 'n', long)]
    dry_run: bool,
    /// Don't ask: rename, adding _2, _3… when a name is taken
    #[arg(short, long)]
    yes: bool,
    /// Put back the names from the last rename, or from the batch with this ID
    #[arg(long, value_name = "ID", num_args = 0..=1, default_missing_value = "")]
    undo: Option<String>,
    /// Finish a rename that was interrupted (the newest, or the one with this ID)
    #[arg(long, value_name = "ID", num_args = 0..=1, default_missing_value = "")]
    resume: Option<String>,
    /// List the renames that can be undone
    #[arg(long)]
    history: bool,
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

/// The history lives in "DateFix data" next to datefix-cli.exe, or in the
/// temp folder if that folder can't be written to.
fn history() -> History {
    let data = data_dir_next_to_exe().unwrap_or_else(|| std::env::temp_dir().join(DATA_FOLDER));
    History::open(&data)
}

fn report(outcome: &Outcome) {
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

/// Success only if nothing failed.
fn exit_code(outcomes: &[Outcome]) -> ExitCode {
    if outcomes.iter().any(|o| matches!(o, Outcome::Failed { .. })) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn when(batch: &Batch) -> String {
    chrono::DateTime::from_timestamp(batch.started as i64, 0)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn describe(batch: &Batch) -> String {
    let state = match batch.state() {
        BatchState::Interrupted => "interrupted".to_string(),
        BatchState::Complete => "can be undone".to_string(),
        BatchState::PartlyUndone => format!("{} put back", batch.undone()),
        BatchState::Undone => "undone".to_string(),
    };
    let folders: Vec<String> = batch
        .folders
        .iter()
        .map(|f| f.display().to_string())
        .collect();
    format!(
        "{}  {}  {} renamed, {state}  {}",
        batch.id,
        when(batch),
        batch.renamed(),
        folders.join(", ")
    )
}

/// Finds a batch by ID, or the default one when the ID is empty.
fn pick(
    history: &History,
    id: &str,
    default: impl FnOnce(&History) -> Option<Batch>,
) -> Option<Batch> {
    if id.is_empty() {
        default(history)
    } else {
        history.get(id)
    }
}

fn show_history() -> ExitCode {
    let batches = history().list();
    if batches.is_empty() {
        println!("No renames in the history yet.");
    }
    for batch in &batches {
        println!("{}", describe(batch));
    }
    ExitCode::SUCCESS
}

fn undo(id: &str) -> ExitCode {
    let Some(mut batch) = pick(&history(), id, History::latest_undoable) else {
        println!("Nothing to undo.");
        return ExitCode::SUCCESS;
    };
    println!("Undoing {}\n", describe(&batch));
    match batch.undo(safe_to_undo) {
        Ok(outcomes) => {
            outcomes.iter().for_each(report);
            let done = outcomes.iter().filter(|o| o.is_renamed()).count();
            println!("\nPut back {done} of {} names.", outcomes.len());
            if batch.can_undo() {
                println!(
                    "The rest stay in the history: fix what's listed above, then run --undo {} again.",
                    batch.id
                );
            }
            exit_code(&outcomes)
        }
        Err(e) => {
            eprintln!("Couldn't update the rename history: {e}");
            ExitCode::FAILURE
        }
    }
}

fn resume(id: &str, on_clash: OnClash) -> ExitCode {
    let Some(mut batch) = pick(&history(), id, |h| h.interrupted().into_iter().next()) else {
        println!("No interrupted rename to finish.");
        return ExitCode::SUCCESS;
    };
    if batch.finished.is_some() {
        println!("{} already finished.", batch.id);
        return ExitCode::SUCCESS;
    }
    println!("Finishing {}\n", describe(&batch));
    match batch.resume(on_clash) {
        Ok(outcomes) => {
            outcomes.iter().for_each(report);
            println!(
                "\nRenamed {} more.",
                outcomes.iter().filter(|o| o.is_renamed()).count()
            );
            exit_code(&outcomes)
        }
        Err(e) => {
            eprintln!("Couldn't update the rename history: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    if args.history {
        return show_history();
    }
    if let Some(id) = &args.undo {
        return undo(id);
    }
    if let Some(id) = &args.resume {
        let on_clash = if args.yes {
            OnClash::Number
        } else {
            OnClash::Skip
        };
        return resume(id, on_clash);
    }

    let history = history();
    for batch in history.interrupted() {
        println!(
            "Note: a rename on {} stopped part way ({} renamed).\n      Run datefix-cli --resume {} to finish it, or --undo {} to put the names back.\n",
            when(&batch),
            batch.renamed(),
            batch.id,
            batch.id
        );
    }

    let directory = args.directory.unwrap_or_else(|| {
        PathBuf::from(ask("Enter the directory path to scan: ").trim_matches('"'))
    });
    if !directory.is_dir() {
        eprintln!("Invalid directory path!");
        return ExitCode::FAILURE;
    }

    let options = PlanOptions {
        recursive: args.recursive,
        month_first: args.month_first,
        include_dirs: !args.files_only,
    };
    let changes = match plan_with(&directory, options, &mut |_, _| true) {
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

    let on_clash = |change: &datefix_core::Change| {
        if args.yes || !change.to.exists() {
            return OnClash::Number;
        }
        println!("\nTarget already exists: {}", file_name(&change.to));
        loop {
            match ask("Would you like to:\n1. Keep original name\n2. Add version number\nChoose (1 or 2): ").as_str() {
                "1" => return OnClash::Skip,
                "2" => return OnClash::Number,
                _ => println!("Invalid choice. Please enter 1 or 2."),
            }
        }
    };
    let outcomes = match history.run(std::slice::from_ref(&directory), &changes, on_clash, report) {
        Ok((_, outcomes)) => outcomes,
        Err(e) => {
            eprintln!("\nNothing was renamed: couldn't save the rename history ({e}).");
            return ExitCode::FAILURE;
        }
    };

    let done = outcomes.iter().filter(|o| o.is_renamed()).count();
    if done > 0 {
        println!("\nChanged your mind? Run: datefix-cli --undo");
    }
    println!(
        "\nCompleted! Successfully renamed {done} out of {} items.",
        changes.len()
    );
    exit_code(&outcomes)
}
