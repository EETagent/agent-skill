use anyhow::{bail, Context, Result};

use super::{
    args::{ChangeArgs, Cli, Command, FindArgs, InstallArgs, ListArgs, PathsArgs, RemoveArgs},
    output::{
        print_catalog_warnings, print_installable_table, print_remote_table, print_root_table,
        print_skill_table, safe_path, write_json, CatalogOutput, FindOutput, RootOutput,
        SourceOutput,
    },
};
use crate::{
    manager::{InstallTarget, SkillManager, SkillQuery},
    model::{InstallableSkill, SkillState},
    source::redact_source,
    terminal::sanitize_inline,
    tui,
};

pub fn run(cli: Cli) -> Result<()> {
    let manager =
        SkillManager::with_roots(cli.project_root, cli.scan_roots, cli.scan_global_roots)?;

    match cli.command {
        None | Some(Command::Tui) => tui::run(manager),
        Some(Command::List(args)) => run_list(&manager, args),
        Some(Command::Enable(args)) => run_change(&manager, args, SkillState::Enabled),
        Some(Command::Disable(args)) => run_change(&manager, args, SkillState::Disabled),
        Some(Command::Install(args)) => run_install(&manager, args),
        Some(Command::Remove(args)) => run_remove(&manager, args),
        Some(Command::Find(args)) => run_find(&manager, args),
        Some(Command::Paths(args)) => run_paths(&manager, args),
    }
}

fn run_list(manager: &SkillManager, args: ListArgs) -> Result<()> {
    let catalog = manager.refresh();
    let query = SkillQuery {
        text: args.search.unwrap_or_default(),
        scope: args.scope.option(),
        state: args.state.option(),
        root: args.root,
    };
    let skills = query.ranked(&catalog);

    if args.json {
        write_json(&CatalogOutput::new(&catalog, &skills))?;
    } else {
        print_skill_table(&skills, manager);
        print_catalog_warnings(&catalog);
    }
    Ok(())
}

fn run_change(manager: &SkillManager, args: ChangeArgs, target: SkillState) -> Result<()> {
    validate_bulk_selection(args.all, &args.selectors, args.yes)?;
    let catalog = manager.refresh();
    let query = SkillQuery {
        text: args.search.unwrap_or_default(),
        scope: args.scope.option(),
        state: Some(target.opposite()),
        root: args.root,
    };
    let selected = manager.select(&catalog, &args.selectors, args.all, &query)?;
    let changes = manager.plan_state_change(&selected, target)?;

    if changes.is_empty() {
        if args.json {
            write_json(&changes)?;
        } else {
            println!("No {} skills matched.", target.opposite());
        }
        return Ok(());
    }

    if !args.dry_run {
        manager.apply_state_change(&changes)?;
    }

    if args.json {
        write_json(&changes)?;
    } else {
        let prefix = if args.dry_run { "Would move" } else { "Moved" };
        for change in &changes {
            println!(
                "{prefix} {}\n  {}\n  -> {}",
                sanitize_inline(&change.skill),
                safe_path(&change.from),
                safe_path(&change.to)
            );
        }
        println!(
            "{} {} skill(s).",
            if args.dry_run {
                format!("Planned to make {target}")
            } else {
                format!("Made {target}")
            },
            changes.len()
        );
    }
    Ok(())
}

fn run_install(manager: &SkillManager, args: InstallArgs) -> Result<()> {
    if args.all && !args.skills.is_empty() {
        bail!("--all cannot be combined with --skill");
    }
    if args.target.is_some() && !args.agents.is_empty() {
        bail!("--target cannot be combined with --agent");
    }

    let prepared = manager
        .prepare_source(&args.source, args.full_depth)
        .with_context(|| format!("could not prepare source {:?}", redact_source(&args.source)))?;

    if args.list {
        if args.json {
            write_json(&SourceOutput::from_prepared(&prepared))?;
        } else {
            println!(
                "Source: {}",
                sanitize_inline(&redact_source(&prepared.resolved))
            );
            print_installable_table(&prepared.skills);
        }
        return Ok(());
    }

    let selected_indices = select_install_indices(&prepared.skills, &args.skills, args.all)?;
    let target = InstallTarget {
        scope: args.scope.into(),
        agents: args.agents,
        custom_root: args.target,
        replace: args.replace,
    };
    let report = manager.install(&prepared, &selected_indices, &target)?;

    if args.json {
        write_json(&report)?;
    } else {
        for change in &report.installed {
            println!(
                "Installed {} -> {}",
                sanitize_inline(&change.title),
                safe_path(&change.destination)
            );
            if let Some(backup) = &change.replaced {
                println!("  archived previous version at {}", safe_path(backup));
            }
        }
        println!("Installed {} skill copy/copies.", report.installed.len());
    }
    Ok(())
}

fn run_remove(manager: &SkillManager, args: RemoveArgs) -> Result<()> {
    validate_bulk_selection(args.all, &args.selectors, args.yes)?;
    if args.purge && !args.yes {
        bail!("--purge permanently deletes data; pass --yes to acknowledge it");
    }

    let catalog = manager.refresh();
    let query = SkillQuery {
        text: args.search.unwrap_or_default(),
        scope: args.scope.option(),
        state: args.state.option(),
        root: args.root,
    };
    let selected = manager.select(&catalog, &args.selectors, args.all, &query)?;
    let changes = manager.plan_remove(&selected, args.purge)?;

    if !args.dry_run {
        manager.apply_remove(&changes)?;
    }

    if args.json {
        write_json(&changes)?;
    } else if changes.is_empty() {
        println!("No skills matched.");
    } else {
        for change in &changes {
            if change.purged {
                println!(
                    "{} {}",
                    if args.dry_run {
                        "Would purge"
                    } else {
                        "Purged"
                    },
                    safe_path(&change.from)
                );
            } else if let Some(destination) = &change.destination {
                println!(
                    "{} {}\n  -> {}",
                    if args.dry_run {
                        "Would remove"
                    } else {
                        "Removed"
                    },
                    safe_path(&change.from),
                    safe_path(destination)
                );
            }
        }
        if !args.purge {
            println!("Removed skills remain recoverable in their .*-trash stores.");
        }
    }
    Ok(())
}

fn run_find(manager: &SkillManager, args: FindArgs) -> Result<()> {
    let query_text = args.query.join(" ");
    let local_query = SkillQuery {
        text: query_text.clone(),
        scope: args.scope.option(),
        state: args.state.option(),
        root: args.root,
    };
    // Remote-only lookup must not touch registered local roots, which may be
    // large, unavailable, or backed by a network filesystem.
    let catalog = (!args.remote_only).then(|| manager.refresh());
    let local = catalog
        .as_ref()
        .map(|catalog| local_query.ranked(catalog))
        .unwrap_or_default();

    let (remote, remote_error) = if args.local_only {
        (Vec::new(), None)
    } else {
        match manager.search_remote(&query_text, args.owner.as_deref(), args.limit) {
            Ok(skills) => (skills, None),
            Err(error) if args.remote_only => return Err(error),
            Err(error) => (Vec::new(), Some(format!("{error:#}"))),
        }
    };

    if args.json {
        write_json(&FindOutput::new(query_text, &local, &remote, remote_error))?;
        return Ok(());
    }

    if !args.remote_only {
        println!("Installed skills ({})", local.len());
        print_skill_table(&local, manager);
        println!();
    }
    if !args.local_only {
        println!("Remote skills ({})", remote.len());
        print_remote_table(&remote);
    }
    if let Some(error) = remote_error {
        eprintln!("warning: remote search failed: {}", sanitize_inline(&error));
    }
    if local.is_empty() && remote.is_empty() {
        println!("No skills matched {query_text:?}.");
    }
    Ok(())
}

fn run_paths(manager: &SkillManager, args: PathsArgs) -> Result<()> {
    let environment = manager.registry().environment();
    let roots = manager
        .registry()
        .roots()
        .iter()
        .filter(|root| args.scope.option().is_none_or(|scope| root.scope == scope))
        .filter(|root| !args.existing || root.path.exists() || root.disabled_path.exists())
        .map(RootOutput::from)
        .collect::<Vec<_>>();

    if args.json {
        write_json(&roots)?;
    } else {
        print_root_table(&roots, environment);
    }
    Ok(())
}

fn validate_bulk_selection(all: bool, selectors: &[String], yes: bool) -> Result<()> {
    if all && !selectors.is_empty() {
        bail!("--all cannot be combined with explicit skill selectors");
    }
    if all && !yes {
        bail!("bulk operations require --yes");
    }
    Ok(())
}

fn select_install_indices(
    skills: &[InstallableSkill],
    requested: &[String],
    all: bool,
) -> Result<Vec<usize>> {
    if all {
        return Ok((0..skills.len()).collect());
    }

    if requested.is_empty() {
        return match skills.len() {
            0 => bail!("source contains no skills"),
            1 => Ok(vec![0]),
            _ => bail!(
                "source contains multiple skills; choose with --skill or pass --all:\n{}",
                skills
                    .iter()
                    .map(|skill| format!("  - {} ({})", skill.title, skill.name))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        };
    }

    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for name in requested {
        let matches = skills
            .iter()
            .enumerate()
            .filter(|(_, skill)| skill.exact_name_match(name))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            missing.push(name.clone());
        } else {
            selected.extend(matches);
        }
    }
    selected.sort_unstable();
    selected.dedup();

    if !missing.is_empty() {
        bail!(
            "source does not contain: {}",
            missing
                .iter()
                .map(|name| format!("{name:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(selected)
}
