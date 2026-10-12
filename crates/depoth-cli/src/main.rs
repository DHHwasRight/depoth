//! `depoth` -- the command line over the framework.
//!
//! Five commands, one output layer. The layer is the interesting part and
//! lives in [`ui`]: whether a person is watching is decided once, in
//! [`ui::Face::decide`], and every byte either command writes goes through a
//! [`ui::Ui`] carrying that answer. No command calls `println!`, checks
//! `is_terminal`, or reads `NO_COLOR`.

mod commands;
mod project;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use ui::{Face, Ui};

/// clap's own help, in the same five styles as everything else. A tool whose
/// `--help` looks like a different program is a tool with two designs.
fn help_styles() -> clap::builder::Styles {
    use anstyle::Style;
    clap::builder::Styles::styled()
        .header(ui::STRONG)
        .usage(ui::STRONG)
        .literal(ui::ACCENT)
        .placeholder(ui::DIM)
        .valid(ui::OK)
        .invalid(ui::WARN)
        .error(ui::BAD.bold())
        .context(Style::new())
}

#[derive(Parser)]
#[command(
    name = "depoth",
    version,
    about = "A static content framework. Templates compile; content does not.",
    styles = help_styles(),
    max_term_width = 100,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    global: Global,
}

/// The flags that feed the one decision, plus where the site is.
#[derive(Args)]
struct Global {
    /// Site root [default: the nearest directory holding a depoth.yaml]
    #[arg(long, short = 'C', global = true, value_name = "DIR")]
    dir: Option<PathBuf>,
    /// Never colour the output, even on a terminal
    #[arg(long, global = true)]
    no_color: bool,
    /// Print only the result and anything wrong with it
    #[arg(long, short, global = true)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new site
    New(commands::new::NewArgs),
    /// Build the site
    Build(commands::build::BuildArgs),
    /// Check the site and its built output
    Check(commands::check::CheckArgs),
    /// Serve the site and rebuild as it changes
    Dev(commands::dev::DevArgs),
    /// Print the resolved navigation as a tree
    Routes(commands::routes::RoutesArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // Asked once. Everything downstream reads the answer off the `Ui`.
    let face = Face::decide(cli.global.no_color, cli.global.quiet);
    let ui = Ui::new(face, cli.global.quiet);
    let dir = cli.global.dir.as_deref();

    let result = match cli.command {
        Command::New(args) => commands::new::run(&ui, args),
        Command::Build(args) => commands::build::run(&ui, dir, args),
        Command::Check(args) => commands::check::run(&ui, dir, args),
        Command::Dev(args) => commands::dev::run(&ui, dir, args),
        Command::Routes(args) => commands::routes::run(&ui, dir, args),
    };

    match result {
        Ok(code) => code,
        Err(diagnostics) => {
            for d in &diagnostics {
                ui.diagnostic(d);
            }
            ExitCode::FAILURE
        }
    }
}
