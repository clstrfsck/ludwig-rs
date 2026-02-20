use clap::Parser;
use std::fs;
use std::io::{self, IsTerminal, Read};

use ludwig::app::App;
use ludwig::screen::Screen;
use ludwig::terminal::{CrosstermTerminal, Terminal};
use ludwig::{Editor, ExecOutcome, compile};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// File to edit
    #[arg(value_name = "FILE")]
    file: Option<String>,

    /// Create a file if it does not exist
    #[arg(short = 'c', long)]
    create: bool,

    /// No initialisation file
    #[arg(short = 'I', long)]
    init_stdin: bool,

    /// Initialisation file
    #[arg(short = 'i', long, value_name = "FILE")]
    init: Option<String>,

    /// Batch mode
    #[arg(short = 'M', long)]
    batch: bool,

    /// Use new command names
    #[arg(short = 'O', long)]
    new_cmds: bool,

    /// Open in read-only mode
    #[arg(short = 'r', long)]
    read_only: bool,
}

fn main() {
    let args = Args::parse();

    // Determine whether to run in interactive or batch mode.
    // Interactive mode: stdin is a terminal AND -M (batch) was not specified.
    let interactive = io::stdin().is_terminal() && !args.batch;

    let maybe_path = args.file.map(|s| {
        if std::path::Path::new(&s).exists() {
            fs::canonicalize(&s).unwrap().to_string_lossy().to_string()
        } else {
            s
        }
    });

    if interactive {
        run_interactive(maybe_path);
    } else {
        run_batch(maybe_path);
    }
}

fn run_interactive(maybe_path: Option<String>) {
    let file_contents = if let Some(path) = maybe_path.as_ref() {
        if std::path::Path::new(path).exists() {
            fs::read_to_string(path).unwrap_or_else(|err| {
                eprintln!("Failed to read {}: {}", path, err);
                std::process::exit(1);
            })
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let editor = Editor::from_str(&file_contents);
    let mut terminal = CrosstermTerminal::new();
    let screen = Screen::new(terminal.size());
    let mut app = App::new(editor, screen, maybe_path);

    if let Err(e) = app.run(&mut terminal) {
        // Make sure terminal is cleaned up even on error
        let _ = terminal.cleanup();
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_batch(maybe_path: Option<String>) {
    let mut output = Vec::<String>::new();

    let file_contents = if let Some(path) = maybe_path.as_ref() {
        let file_contents = fs::read_to_string(path).unwrap_or_else(|err| {
            eprintln!("Failed to read {}: {}", path, err);
            std::process::exit(1);
        });
        output.push(format!(
            "{} closed ({} line{} read).",
            path,
            file_contents.lines().count(),
            if file_contents.lines().count() == 1 {
                ""
            } else {
                "s"
            }
        ));
        file_contents
    } else {
        String::new()
    };

    let mut stdin_contents = String::new();
    io::stdin()
        .read_to_string(&mut stdin_contents)
        .unwrap_or_else(|err| {
            eprintln!("Failed to read stdin: {}", err);
            std::process::exit(1);
        });

    let code = compile(&stdin_contents).unwrap_or_else(|err| {
        println!("{}", err);
        for line in output.clone() {
            println!("{}", line);
        }
        std::process::exit(0);
    });

    let mut editor = Editor::from_str(&file_contents);
    let outcome = editor.execute(&code);

    let failed = !matches!(
        outcome,
        ExecOutcome::Success | ExecOutcome::ExitSuccess { .. } | ExecOutcome::ExitSuccessAll
    );
    if failed {
        println!("\x07COMMAND FAILED");
    }

    for line in output {
        println!("{}", line);
    }
    if !failed
        && editor.modified()
        && let Some(path) = maybe_path.as_ref()
    {
        fs::rename(path, format!("{}~1", path)).unwrap();
        let mut contents = editor.to_string();
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        println!(
            "{} created ({} line{} written).",
            path,
            contents.lines().count(),
            if contents.lines().count() == 1 {
                ""
            } else {
                "s"
            }
        );
        fs::write(path, contents).unwrap();
    }
}
