//! `depoth dev` -- not yet the dev server.
//!
//! The server itself belongs to another crate (`depoth::dev`) and another
//! author. What is settled is the shape, and printing the shape is worth more
//! than printing "not implemented": it tells a reader which half of the build
//! their edit will pay for, which is the one thing the two-clock design asks
//! them to know.

use std::path::Path;
use std::process::ExitCode;

use clap::Args;

use crate::commands::Outcome;
use crate::project::Project;
use crate::ui::{Cell, Table, Ui, ACCENT, DIM, STRONG};

#[derive(Args)]
pub struct DevArgs {
    /// Port to listen on
    #[arg(long, default_value_t = 3000, value_name = "PORT")]
    pub port: u16,
    /// Deployment sub-path, e.g. /depoth
    #[arg(long, value_name = "PATH")]
    pub base: Option<String>,
}

pub fn run(ui: &Ui, dir: Option<&Path>, args: DevArgs) -> Outcome {
    let project = Project::discover(dir)?;
    ui.heading("dev", project.display(&project.config_path));

    ui.note(format!(
        "  {}",
        crate::ui::paint(STRONG, "Not built yet. When it is, this command will:")
    ));
    ui.gap();

    let mut table = Table::new().indent(4);
    let mut row = |what: &str, detail: &str| {
        table.row(vec![
            Cell::styled(what.to_string(), ACCENT),
            Cell::styled(detail.to_string(), DIM),
        ]);
    };
    row(
        "serve",
        &format!(
            "http://127.0.0.1:{} from memory, never from disk",
            args.port
        ),
    );
    row("watch", "content/, assets/, public/ and depoth.yaml");
    row("reload", "over SSE, so the browser reconnects by itself");
    row(
        "rebuild",
        "content edits only; a template edit asks for cargo",
    );
    row("survive", "a failing build leaves the watcher running");
    ui.table_note(&table);

    ui.gap();
    ui.note(format!(
        "  {}",
        crate::ui::paint(
            DIM,
            "Never --release: a release build is around 50 s and nothing \
             about authoring needs it."
        )
    ));
    ui.gap();
    ui.warn(format!(
        "`depoth dev` is a placeholder; run {} for now",
        crate::ui::paint(ACCENT, "depoth build")
    ));
    // Not a failure: the command did exactly what it says it does. A non-zero
    // exit here would break a script that merely probes the surface.
    Ok(ExitCode::SUCCESS)
}
