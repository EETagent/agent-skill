use agent_skill::cli::{Cli, Command};
use clap::Parser;

#[test]
fn defaults_to_the_interactive_interface() {
    let cli = Cli::try_parse_from(["skillctl"]).expect("parse CLI");
    assert!(cli.command.is_none());
}

#[test]
fn parses_bulk_disable_filters() {
    let cli = Cli::try_parse_from([
        "skillctl", "disable", "--all", "--yes", "--scope", "global", "--search", "review",
        "--root", "codex",
    ])
    .expect("parse disable command");

    match cli.command {
        Some(Command::Disable(args)) => {
            assert!(args.all);
            assert!(args.yes);
            assert_eq!(args.search.as_deref(), Some("review"));
            assert_eq!(args.root.as_deref(), Some("codex"));
        }
        _ => panic!("expected disable command"),
    }
}

#[test]
fn parses_install_source_selection_and_target() {
    let cli = Cli::try_parse_from([
        "skillctl",
        "install",
        "owner/repo",
        "--skill",
        "alpha",
        "--agent",
        "claude-code",
        "--scope",
        "global",
        "--replace",
    ])
    .expect("parse install command");

    match cli.command {
        Some(Command::Install(args)) => {
            assert_eq!(args.source, "owner/repo");
            assert_eq!(args.skills, vec!["alpha".to_owned()]);
            assert_eq!(args.agents, vec!["claude-code".to_owned()]);
            assert!(args.replace);
        }
        _ => panic!("expected install command"),
    }
}
