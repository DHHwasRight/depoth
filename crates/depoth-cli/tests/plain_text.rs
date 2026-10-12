//! The one decision, asserted at the binary rather than at the layer.
//!
//! `src/ui.rs` has unit tests over a `Ui` built on an in-memory writer, which
//! proves the renderers behave. These run the real executable with no terminal
//! on either descriptor -- which is what CI, a pipe and a redirect all look
//! like -- and prove that nothing downstream of the decision reintroduces an
//! escape sequence or changes a single character of layout.
//!
//! Only commands that need no compiler are used, so the suite stays in
//! milliseconds: `routes` and `check --sources-only`.

use std::path::{Path, PathBuf};
use std::process::Command;

const DEPOTH: &str = env!("CARGO_BIN_EXE_depoth");

/// A site on disk. Small, but with every shape `routes` can print: a tab, a
/// nested group, a group root, and a page the navigation names but nobody
/// wrote.
fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("depoth-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("content/guides")).unwrap();
    std::fs::write(
        root.join("depoth.yaml"),
        "\
name: Fixture
base_url: https://example.com
navigation:
  anchors:
    - anchor: Source
      href: https://example.com/src
  tabs:
    - tab: Guide
      groups:
        - group: Start
          root: index
          pages:
            - install
        - group: Deeper
          pages:
            - group: Guides
              pages: [guides/first, guides/ghost]
",
    )
    .unwrap();
    for (path, title) in [
        ("content/index.md", "Home"),
        ("content/install.md", "Install"),
        ("content/guides/first.md", "First"),
    ] {
        std::fs::write(
            root.join(path),
            format!("---\ntitle: {title}\n---\n\n# {title}\n"),
        )
        .unwrap();
    }
    root
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

impl Run {
    /// Both streams in the order a reader would have met them. `routes`
    /// writes its table to stdout and its commentary to stderr, so comparing
    /// only one of them would miss half the transcript.
    fn whole(&self) -> String {
        format!("{}{}", self.stderr, self.stdout)
    }
}

fn run(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut cmd = Command::new(DEPOTH);
    cmd.arg("-C").arg(root).args(args);
    // A clean slate: the host's own NO_COLOR must not decide the outcome of a
    // test about NO_COLOR.
    cmd.env_remove("NO_COLOR").env_remove("CLICOLOR_FORCE");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("run depoth");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code().unwrap_or(-1),
    }
}

fn strip(text: &str) -> String {
    anstream::adapter::strip_str(text).to_string()
}

#[test]
fn a_pipe_receives_no_escape_sequence_at_all() {
    let root = fixture("plain");
    for args in [
        vec!["routes"],
        vec!["routes", "--flat"],
        vec!["check", "--sources-only"],
        vec!["dev"],
    ] {
        let run = run(&root, &args, &[]);
        let whole = run.whole();
        assert!(
            !whole.contains('\u{1b}'),
            "{args:?} wrote an escape sequence to a pipe:\n{whole:?}"
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_coloured_transcript_is_the_plain_one_with_colour_added() {
    // The claim the whole layer rests on: one renderer, two transcripts, and
    // the difference between them is exactly the escape sequences. If a
    // command ever branched on the face to lay something out differently,
    // this is the test that would catch it.
    let root = fixture("same");
    for args in [
        vec!["routes"],
        vec!["routes", "--flat"],
        vec!["check", "--sources-only"],
    ] {
        let plain = run(&root, &args, &[]);
        let coloured = run(&root, &args, &[("CLICOLOR_FORCE", "1")]);
        assert!(
            coloured.whole().contains('\u{1b}'),
            "{args:?} should have coloured under CLICOLOR_FORCE"
        );
        assert_eq!(
            strip(&coloured.whole()),
            plain.whole(),
            "{args:?} differs by more than colour"
        );
        assert_eq!(coloured.code, plain.code, "{args:?} exit code differs");
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn no_color_the_flag_and_the_variable_agree() {
    let root = fixture("nocolor");
    let flag = run(&root, &["routes", "--no-color"], &[("CLICOLOR_FORCE", "1")]);
    let variable = run(
        &root,
        &["routes"],
        &[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")],
    );
    let piped = run(&root, &["routes"], &[]);
    assert_eq!(flag.whole(), piped.whole());
    assert_eq!(variable.whole(), piped.whole());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn quiet_keeps_the_table_and_drops_the_commentary() {
    let root = fixture("quiet");
    let loud = run(&root, &["routes"], &[]);
    let quiet = run(&root, &["routes", "--quiet"], &[]);
    assert_eq!(quiet.stderr, "", "--quiet should say nothing on stderr");
    assert_eq!(
        quiet.stdout, loud.stdout,
        "--quiet must not change what was asked for"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn routes_prints_the_tree_whatever_is_wrong_with_it() {
    let root = fixture("tree");
    let run = run(&root, &["routes"], &[]);
    assert_eq!(run.code, 0, "routes reports, it does not judge");
    let stdout = run.stdout;
    // The tree: a tab, two groups, a nested group, and the rules between.
    for line in [
        "Guide",
        "├─ Start",
        "│  ├─ index     /              content/index.md",
        "│  └─ install   /install       content/install.md",
        "└─ Deeper",
        "   └─ Guides",
        "      ├─ first  /guides/first  content/guides/first.md",
        "      └─ ghost  /guides/ghost  not written",
    ] {
        assert!(stdout.contains(line), "missing {line:?} in:\n{stdout}");
    }
    assert!(stdout.contains("Source  https://example.com/src"));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn check_exits_non_zero_and_says_where_in_rustc_shape() {
    let root = fixture("check");
    let run = run(&root, &["check", "--sources-only"], &[]);
    assert_eq!(run.code, 1, "a finding must fail the command");
    assert!(
        run.stderr.contains(
            "error[nav/not-written]: guides/ghost is in the navigation, but no file was found"
        ),
        "{}",
        run.stderr
    );
    assert!(run.stderr.contains(" --> depoth.yaml"), "{}", run.stderr);
    assert!(
        run.stderr
            .contains("  = help: write content/guides/ghost.md"),
        "{}",
        run.stderr
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_frontmatter_fault_is_quoted_with_a_caret() {
    let root = fixture("front");
    std::fs::write(
        root.join("content/install.md"),
        "---\ntitel: Install\n---\n\n# Install\n",
    )
    .unwrap();
    let run = run(&root, &["check", "--sources-only"], &[]);
    assert_eq!(run.code, 1);
    for line in [
        "error[frontmatter]: missing field `title`",
        " --> content/install.md:2:1",
        "  |",
        "2 | titel: Install",
        "  | ^",
    ] {
        assert!(
            run.stderr.contains(line),
            "missing {line:?} in:\n{}",
            run.stderr
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_missing_name_names_the_argument_rather_than_waiting_for_one() {
    // The whole reason every prompt has a flag. With no terminal this must
    // fail immediately; a prompt here would hang a CI job until it timed out.
    let dir = std::env::temp_dir();
    let out = Command::new(DEPOTH)
        .arg("new")
        .current_dir(&dir)
        .output()
        .expect("run depoth");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no name, and no terminal to ask for one"),
        "{stderr}"
    );
    assert!(
        stderr.contains("= help: pass it: `depoth new <NAME>`"),
        "{stderr}"
    );
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn a_bad_config_key_is_quoted_from_the_file() {
    let root = fixture("badkey");
    std::fs::write(
        root.join("depoth.yaml"),
        "name: Fixture\nbase_ur1: https://example.com\nnavigation:\n  pages: [index]\n",
    )
    .unwrap();
    let run = run(&root, &["routes"], &[]);
    assert_eq!(run.code, 1);
    assert!(
        run.stderr
            .contains("error[config]: unknown field `base_ur1`"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("2 | base_ur1: https://example.com"),
        "{}",
        run.stderr
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_navigation_that_breaks_the_one_child_rule_says_so() {
    let root = fixture("twokinds");
    std::fs::write(
        root.join("depoth.yaml"),
        "name: Fixture\nnavigation:\n  groups: [{group: A, pages: [index]}]\n  pages: [install]\n",
    )
    .unwrap();
    let run = run(&root, &["routes"], &[]);
    assert_eq!(run.code, 1);
    assert!(run.stderr.contains("error[navigation]:"), "{}", run.stderr);
    assert!(
        run.stderr
            .contains("a node carries exactly one kind of child"),
        "{}",
        run.stderr
    );
    std::fs::remove_dir_all(&root).unwrap();
}
