# agent-skill (`skillctl`)

`skillctl` is a cross-platform Rust CLI and Ratatui application for managing agent skills stored as directories containing `SKILL.md`.

It discovers project-local and user-global skill roots, previews skill documents, searches installed metadata, enables and disables skills without deleting them, installs skills from local directories or Git repositories, searches the skills catalog, and removes skills through a reversible archive by default.

## Safety model

The normal workflows preserve data:

- **Disable** moves the complete skill directory from an enabled root to a hidden sibling disabled store.
- **Enable** moves it back to its original relative path.
- **Remove** moves it to a timestamped hidden sibling trash store.
- **Install with `--replace`** archives the previous destination in trash before committing the replacement.
- **Only `remove --purge --yes` permanently deletes a skill.**

For a root such as `.agents/skills`, the stores are:

```text
.agents/
├── skills/                         # enabled
│   └── rust-review/
├── .skills-disabled/               # disabled, same relative layout
│   └── rust-review/
└── .skills-trash/                  # removals and replaced versions
    └── 20260809T120000.000Z/
        ├── enabled/rust-review/
        └── replaced/rust-review/
```

Disabled skills are kept outside the enabled container. This avoids relying on every agent to ignore a nested `.disabled` directory during recursive discovery.

## Build

The crate targets Rust 1.88 or newer.

```bash
cargo build --release
```

The binary is written to `target/release/skillctl` (`skillctl.exe` on Windows).

## Interactive interface

Run without a subcommand, or use `tui` explicitly:

```bash
skillctl
skillctl tui
```

The main screen shows the discovered skills and a live, syntax-highlighted `SKILL.md` preview. Highlighting is source-preserving: Markdown markers, frontmatter, links, URLs, and fenced-code delimiters remain visible exactly as written after terminal-safety sanitization. Recognized fenced-code languages receive language-specific highlighting in both the main preview and install picker. The layout switches between side-by-side and stacked panes based on terminal width.

| Key | Action |
|---|---|
| `j` / `k`, arrows | Move in the skill list; scroll when Preview has focus |
| `Tab` | Switch focus between list and preview |
| `/` | Edit live full-text search |
| `c` | Clear search |
| `g` / `s` | Cycle scope and state filters |
| `Space` / `Enter` | Toggle the selected skill |
| `e` / `d` | Enable or disable the selected skill |
| `E` / `D` | Enable or disable all currently visible skills, after confirmation |
| `x` / `X` | Remove selected or all visible skills to reversible trash |
| `i` | Inspect and install from a local or Git source |
| `f` | Search the remote catalog, inspect a result, and install it |
| `PageUp` / `PageDown`, `Ctrl+U` / `Ctrl+D` | Scroll preview |
| `r` | Rescan every registered path |
| `?` | Help |
| `q`, `Ctrl+C` | Quit |

The install picker supports `Space` to select, `a` for all/none, `Tab` to switch local/global default targets, `r` to toggle replacement, and `Enter` to install.

## CLI commands

The non-interactive commands use the same `SkillManager`, catalog, query, and filesystem operation code as the TUI.

### List and search installed skills

```bash
skillctl list
skillctl list --search "rust review"
skillctl list --scope global --state disabled
skillctl list --root codex --json
```

Search is case-insensitive and token-based. Every token must match somewhere in the title, name, description, relative path, or associated agent names. Exact title/name matches rank first.

### Enable and disable

Selectors may be a skill ID, exact name, exact title, relative path, or full path.

```bash
skillctl disable rust-review
skillctl enable rust-review
skillctl disable --all --scope local --search review --yes
skillctl enable --all --scope global --root codex --yes
skillctl disable rust-review --dry-run --json
```

`--all` always requires `--yes`. A bulk move is preflighted and rolled back if a later rename fails.

### Install

```bash
# Local directory, repository, or direct SKILL.md path
skillctl install ./my-skills
skillctl install ./my-skills/SKILL.md

# GitHub shorthand and source selection
skillctl install owner/repository
skillctl install owner/repository@skill-name
skillctl install 'owner/repository#branch@skill-name'
skillctl install owner/repository/skills/rust

# Repository URLs
skillctl install https://github.com/owner/repository/tree/main/skills/rust
skillctl install https://gitlab.com/group/repository/-/tree/main/skills/rust
skillctl install git@github.com:owner/repository.git

# Multi-skill source and targets
skillctl install owner/repository --list
skillctl install owner/repository --skill alpha --skill beta
skillctl install owner/repository --all --agent claude-code --agent codex
skillctl install owner/repository --scope global --agent opencode
skillctl install owner/repository --target /custom/skills
skillctl install owner/repository --replace
```

Supported source forms are local paths, `file:` URLs, GitHub/GitLab repository URLs, GitHub owner/repository shorthand, SSH Git URLs, and generic HTTP(S)/Git clone URLs. Remote sources require `git` on `PATH`. `GH_HOST` is honored for GitHub Enterprise shorthand in the same hostname-only form used by GitHub CLI.

By default, source discovery checks the source root, all known agent containers, Claude plugin manifests and conventional plugin `skills/` directories, then a bounded fallback when necessary. `--full-depth` requests the bounded repository-wide scan even after skills were found in known containers.

Installs default to:

- local: `<project>/.agents/skills`
- global: `~/.agents/skills`

Use repeated `--agent` values for agent-specific destinations or `--target` for one explicit skills container. Multiple targets are staged before any destination is committed.

### Remove

```bash
# Reversible archive (default)
skillctl remove rust-review
skillctl remove --all --state disabled --yes

# Inspect the plan
skillctl remove rust-review --dry-run --json

# Irreversible deletion
skillctl remove rust-review --purge --yes
```

Normal removal reports the archive destination. Restoration from trash is intentionally explicit and manual: move the archived skill directory back into its enabled or disabled store after checking for name conflicts. Purge is deliberately not transactional because deleted data cannot be reconstructed.

### Find

```bash
skillctl find rust review
skillctl find rust --local-only
skillctl find rust --remote-only --owner example --limit 10
skillctl find rust --scope global --state enabled --json
```

The default combines local results with the skills catalog. A remote failure does not invalidate local results unless `--remote-only` was requested.

### Inspect registered paths

```bash
skillctl paths
skillctl paths --existing
skillctl paths --scope global --json
```

See [`docs/PATHS.md`](docs/PATHS.md) for the complete registry and alternate layouts.

## Project root and custom scan roots

Local paths are resolved from the detected project root. Detection walks upward for strong project markers such as `.git`, `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, or `skills-lock.json`. It also recognizes local skill markers, except at the home directory where `.agents/skills` is global. When the selected project root is the home directory, duplicate physical roots are merged and retain global classification.

Override discovery explicitly when needed:

```bash
skillctl --project-root /work/project list
skillctl --scan-root /extra/local/skills list
skillctl --scan-global-root /extra/global/skills list
```

Both custom scan flags may be repeated.

## Environment variables

| Variable | Purpose |
|---|---|
| `SKILLS_API_URL` | Override the remote search service base URL |
| `GH_HOST` | GitHub Enterprise hostname used by shorthand sources |
| `XDG_CONFIG_HOME` | XDG configuration root when absolute |
| `CODEX_HOME` | Codex configuration home; `/skills` is appended |
| `CLAUDE_CONFIG_DIR` | Claude configuration home; `/skills` is appended |
| `AUTOHAND_HOME` | Autohand configuration home; `/skills` is appended |
| `GROK_HOME` | Grok configuration home; `/skills` is appended |
| `HERMES_HOME` | Hermes configuration home; `/skills` is appended |
| `VIBE_HOME` | Mistral Vibe configuration home; `/skills` is appended |
| `COLUMNS` | Plain CLI table width hint, minimum 60 |

When an agent uses an environment-specific home, an absolute configured location and its conventional fallback are both scanned. Installation prefers the configured location.

## Filesystem and portability details

- Paths use `Path`/`PathBuf`; Git subprocess arguments are passed as OS strings rather than lossy UTF-8 command strings.
- Windows reserved device names are prefixed during install directory sanitization.
- Source copies reject symlinks that escape the skill directory and detect symlink cycles.
- Installed linked skill directories are manageable as links, but linked containers without their own root `SKILL.md` are not recursively traversed.
- Source subpaths and plugin manifest paths cannot contain absolute, parent, root, or Windows prefix components.
- `SKILL.md` preview reads and remote catalog response bodies are capped at 2 MiB; the install picker renders and caches at most a 32 KiB excerpt.
- Terminal control sequences are removed from metadata, paths, statuses, previews, Git errors, and remote results before display. Source credentials and query values are redacted from human and JSON source fields.
- Enabled/disabled/trash operations use rename semantics. The root and its sibling stores therefore need to be on a filesystem that permits those renames; the default layouts satisfy this.

## JSON output

`list`, `enable`, `disable`, `install --list`, `install`, `remove`, `find`, and `paths` support JSON where applicable. Preview bodies are intentionally omitted from installed-skill list JSON to keep output bounded; metadata, paths, IDs, agents, and warnings remain available. Source strings remain machine-readable but have embedded credentials and query values redacted. Filesystem paths that are not valid UTF-8 are emitted as strings with invalid byte sequences replaced by the Unicode replacement character (`U+FFFD`).

## Design and testing

- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) describes the shared backend, TUI boundaries, and filesystem transaction model.
- [`docs/PATHS.md`](docs/PATHS.md) lists all agent path definitions.
- Integration tests under `tests/` cover parsing and bounded reads, discovery, path aggregation, reversible moves, replacement installs, search, remote input validation, terminal sanitization, and Clap parsing.
- [`FILE_MANIFEST.md`](FILE_MANIFEST.md) inventories the delivered tree; [`STATIC_REVIEW.md`](STATIC_REVIEW.md) records this refactor and its verification limits.

This delivery was reviewed without a Rust toolchain. Static lexical, delimiter, structure, path, Markdown-link, patch, archive, and checksum checks were performed, but `rustfmt`, compilation, Clippy, tests, and the release build were not run. Validate the source with Rust 1.88 or newer before publishing:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
```
