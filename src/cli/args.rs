use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::model::{Scope, SkillState};

#[derive(Debug, Parser)]
#[command(
    name = "skillctl",
    version,
    about = "Manage local and global agent skills",
    long_about = "A cross-platform Clap and Ratatui tool for discovering, previewing, \
enabling, disabling, installing, finding, and safely removing agent skills. Running without a \
subcommand opens the interactive UI."
)]
pub struct Cli {
    /// Project root used for local skill discovery.
    #[arg(long, global = true, value_name = "DIR")]
    pub project_root: Option<PathBuf>,

    /// Additional local skills container to scan. May be repeated.
    #[arg(long = "scan-root", global = true, value_name = "DIR")]
    pub scan_roots: Vec<PathBuf>,

    /// Additional global skills container to scan. May be repeated.
    #[arg(long = "scan-global-root", global = true, value_name = "DIR")]
    pub scan_global_roots: Vec<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the interactive Ratatui interface.
    Tui,
    /// List installed enabled and disabled skills.
    List(ListArgs),
    /// Enable one or more disabled skills.
    Enable(ChangeArgs),
    /// Disable one or more enabled skills without deleting them.
    Disable(ChangeArgs),
    /// Install skills from a local directory or Git repository.
    Install(InstallArgs),
    /// Move skills to reversible trash, or explicitly purge them.
    Remove(RemoveArgs),
    /// Search installed skills and the skills.sh catalog.
    Find(FindArgs),
    /// Show every registered local and global skills path.
    Paths(PathsArgs),
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Full-text search over title, name, description, path, and agent aliases.
    #[arg(short, long, value_name = "QUERY")]
    pub search: Option<String>,

    #[arg(long, value_enum, default_value = "all")]
    pub scope: ScopeFilter,

    #[arg(long, value_enum, default_value = "all")]
    pub state: StateFilter,

    /// Restrict by root ID, path fragment, or agent name.
    #[arg(long, value_name = "ROOT")]
    pub root: Option<String>,

    /// Emit machine-readable JSON without SKILL.md preview bodies.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ChangeArgs {
    /// Skill ID, exact name, title, relative path, or full path.
    #[arg(value_name = "SKILL")]
    pub selectors: Vec<String>,

    /// Select every skill after applying filters.
    #[arg(long)]
    pub all: bool,

    /// Full-text filter used with --all or selectors.
    #[arg(short, long, value_name = "QUERY")]
    pub search: Option<String>,

    #[arg(long, value_enum, default_value = "all")]
    pub scope: ScopeFilter,

    /// Restrict by root ID, path fragment, or agent name.
    #[arg(long, value_name = "ROOT")]
    pub root: Option<String>,

    /// Required for --all as an explicit bulk-operation acknowledgement.
    #[arg(short, long)]
    pub yes: bool,

    /// Show the move plan without changing the filesystem.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit the operation plan/result as JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Local path, Git URL, GitHub/GitLab URL, or owner/repository shorthand.
    pub source: String,

    /// Install a named skill from a multi-skill source. May be repeated.
    #[arg(long = "skill", short = 's', value_name = "NAME")]
    pub skills: Vec<String>,

    /// Install every discovered skill in the source.
    #[arg(long)]
    pub all: bool,

    /// Install into local project paths or global user paths.
    #[arg(long, value_enum, default_value = "local")]
    pub scope: ScopeValue,

    /// Install to an agent-specific path. May be repeated.
    #[arg(long = "agent", short = 'a', value_name = "AGENT")]
    pub agents: Vec<String>,

    /// Install into an explicit skills container instead of a known target.
    #[arg(long, value_name = "DIR")]
    pub target: Option<PathBuf>,

    /// Archive an existing destination in .*-trash before replacing it.
    #[arg(long)]
    pub replace: bool,

    /// Recursively scan the whole source repository, including fallback paths.
    #[arg(long)]
    pub full_depth: bool,

    /// Discover and list source skills without installing.
    #[arg(long)]
    pub list: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Skill ID, exact name, title, relative path, or full path.
    #[arg(value_name = "SKILL")]
    pub selectors: Vec<String>,

    /// Select every skill after applying filters.
    #[arg(long)]
    pub all: bool,

    /// Full-text filter used with --all or selectors.
    #[arg(short, long, value_name = "QUERY")]
    pub search: Option<String>,

    #[arg(long, value_enum, default_value = "all")]
    pub scope: ScopeFilter,

    #[arg(long, value_enum, default_value = "all")]
    pub state: StateFilter,

    /// Restrict by root ID, path fragment, or agent name.
    #[arg(long, value_name = "ROOT")]
    pub root: Option<String>,

    /// Permanently delete instead of moving to the reversible trash store.
    #[arg(long)]
    pub purge: bool,

    /// Required for --all and every --purge operation.
    #[arg(short, long)]
    pub yes: bool,

    /// Show the removal plan without changing the filesystem.
    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct FindArgs {
    /// Search terms. Multiple unquoted terms are joined with spaces.
    #[arg(required = true, num_args = 1.., value_name = "QUERY")]
    pub query: Vec<String>,

    /// Search only installed skills.
    #[arg(long, conflicts_with = "remote_only")]
    pub local_only: bool,

    /// Search only the remote skills catalog.
    #[arg(long, conflicts_with = "local_only")]
    pub remote_only: bool,

    /// Restrict remote results to a GitHub owner.
    #[arg(long, value_name = "OWNER")]
    pub owner: Option<String>,

    /// Maximum remote results.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,

    #[arg(long, value_enum, default_value = "all")]
    pub scope: ScopeFilter,

    #[arg(long, value_enum, default_value = "all")]
    pub state: StateFilter,

    /// Restrict local results by root ID, path fragment, or agent name.
    #[arg(long, value_name = "ROOT")]
    pub root: Option<String>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PathsArgs {
    /// Show only roots whose enabled or disabled store currently exists.
    #[arg(long)]
    pub existing: bool,

    #[arg(long, value_enum, default_value = "all")]
    pub scope: ScopeFilter,

    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ScopeValue {
    Local,
    Global,
}

impl From<ScopeValue> for Scope {
    fn from(value: ScopeValue) -> Self {
        match value {
            ScopeValue::Local => Self::Local,
            ScopeValue::Global => Self::Global,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ScopeFilter {
    All,
    Local,
    Global,
}

impl ScopeFilter {
    pub(super) fn option(self) -> Option<Scope> {
        match self {
            Self::All => None,
            Self::Local => Some(Scope::Local),
            Self::Global => Some(Scope::Global),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StateFilter {
    All,
    Enabled,
    Disabled,
}

impl StateFilter {
    pub(super) fn option(self) -> Option<SkillState> {
        match self {
            Self::All => None,
            Self::Enabled => Some(SkillState::Enabled),
            Self::Disabled => Some(SkillState::Disabled),
        }
    }
}
