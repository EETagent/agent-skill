#[derive(Debug, Clone, Copy)]
pub enum GlobalLocation {
    Home(&'static str),
    Config(&'static str),
    EnvHome {
        variable: &'static str,
        fallback: &'static str,
        suffix: &'static str,
    },
    OpenClaw,
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct AgentSpec {
    pub key: &'static str,
    pub display_name: &'static str,
    pub local_skills_dir: &'static str,
    pub global_skills_dir: GlobalLocation,
}

pub(super) const AGENTS: &[AgentSpec] = &[
    agent(
        "aider-desk",
        "AiderDesk",
        ".aider-desk/skills",
        home(".aider-desk/skills"),
    ),
    agent("amp", "Amp", ".agents/skills", config("agents/skills")),
    agent(
        "antigravity",
        "Antigravity",
        ".agents/skills",
        home(".gemini/antigravity/skills"),
    ),
    agent(
        "antigravity-cli",
        "Antigravity CLI",
        ".agents/skills",
        home(".gemini/antigravity-cli/skills"),
    ),
    agent(
        "astrbot",
        "AstrBot",
        "data/skills",
        home(".astrbot/data/skills"),
    ),
    agent(
        "autohand-code",
        "Autohand Code CLI",
        ".autohand/skills",
        env_home("AUTOHAND_HOME", ".autohand", "skills"),
    ),
    agent(
        "augment",
        "Augment",
        ".augment/skills",
        home(".augment/skills"),
    ),
    agent("bob", "IBM Bob", ".bob/skills", home(".bob/skills")),
    agent(
        "claude-code",
        "Claude Code",
        ".claude/skills",
        env_home("CLAUDE_CONFIG_DIR", ".claude", "skills"),
    ),
    agent("openclaw", "OpenClaw", "skills", GlobalLocation::OpenClaw),
    agent("cline", "Cline", ".agents/skills", home(".agents/skills")),
    agent(
        "codearts-agent",
        "CodeArts Agent",
        ".codeartsdoer/skills",
        home(".codeartsdoer/skills"),
    ),
    agent(
        "codebuddy",
        "CodeBuddy",
        ".codebuddy/skills",
        home(".codebuddy/skills"),
    ),
    agent(
        "codemaker",
        "Codemaker",
        ".codemaker/skills",
        home(".codemaker/skills"),
    ),
    agent(
        "codestudio",
        "Code Studio",
        ".codestudio/skills",
        home(".codestudio/skills"),
    ),
    agent(
        "codex",
        "Codex",
        ".agents/skills",
        env_home("CODEX_HOME", ".codex", "skills"),
    ),
    agent(
        "command-code",
        "Command Code",
        ".commandcode/skills",
        home(".commandcode/skills"),
    ),
    agent(
        "continue",
        "Continue",
        ".continue/skills",
        home(".continue/skills"),
    ),
    agent(
        "cortex",
        "Cortex Code",
        ".cortex/skills",
        home(".snowflake/cortex/skills"),
    ),
    agent(
        "crush",
        "Crush",
        ".crush/skills",
        home(".config/crush/skills"),
    ),
    agent("cursor", "Cursor", ".agents/skills", home(".cursor/skills")),
    agent(
        "deepagents",
        "Deep Agents",
        ".agents/skills",
        home(".deepagents/agent/skills"),
    ),
    agent(
        "devin",
        "Devin for Terminal",
        ".devin/skills",
        config("devin/skills"),
    ),
    agent("dexto", "Dexto", ".agents/skills", home(".agents/skills")),
    agent("droid", "Droid", ".factory/skills", home(".factory/skills")),
    agent("eve", "Eve", "agent/skills", GlobalLocation::None),
    agent(
        "firebender",
        "Firebender",
        ".agents/skills",
        home(".firebender/skills"),
    ),
    agent(
        "forgecode",
        "ForgeCode",
        ".forge/skills",
        home(".forge/skills"),
    ),
    agent(
        "gemini-cli",
        "Gemini CLI",
        ".agents/skills",
        home(".gemini/skills"),
    ),
    agent(
        "github-copilot",
        "GitHub Copilot",
        ".agents/skills",
        home(".copilot/skills"),
    ),
    agent("goose", "Goose", ".goose/skills", config("goose/skills")),
    agent(
        "grok",
        "Grok Build",
        ".grok/skills",
        env_home("GROK_HOME", ".grok", "skills"),
    ),
    agent(
        "hermes-agent",
        "Hermes Agent",
        ".hermes/skills",
        env_home("HERMES_HOME", ".hermes", "skills"),
    ),
    agent(
        "inference-sh",
        "inference.sh",
        ".inferencesh/skills",
        home(".inferencesh/skills"),
    ),
    agent("jazz", "Jazz", ".jazz/skills", home(".jazz/skills")),
    agent("junie", "Junie", ".junie/skills", home(".junie/skills")),
    agent(
        "iflow-cli",
        "iFlow CLI",
        ".iflow/skills",
        home(".iflow/skills"),
    ),
    agent(
        "kilo",
        "Kilo Code",
        ".kilocode/skills",
        home(".kilocode/skills"),
    ),
    agent(
        "kimchi",
        "Kimchi",
        ".kimchi/skills",
        home(".config/kimchi/harness/skills"),
    ),
    agent(
        "kimi-code-cli",
        "Kimi Code CLI",
        ".agents/skills",
        home(".agents/skills"),
    ),
    agent("kiro-cli", "Kiro CLI", ".kiro/skills", home(".kiro/skills")),
    agent("kode", "Kode", ".kode/skills", home(".kode/skills")),
    agent("lingma", "Lingma", ".lingma/skills", home(".lingma/skills")),
    agent("loaf", "Loaf", ".agents/skills", home(".agents/skills")),
    agent("mcpjam", "MCPJam", ".mcpjam/skills", home(".mcpjam/skills")),
    agent(
        "minimax-code",
        "MiniMax Code",
        ".minimax/skills",
        home(".minimax/skills"),
    ),
    agent(
        "mistral-vibe",
        "Mistral Vibe",
        ".vibe/skills",
        env_home("VIBE_HOME", ".vibe", "skills"),
    ),
    agent("moxby", "Moxby", ".moxby/skills", home(".moxby/skills")),
    agent("mux", "Mux", ".mux/skills", home(".mux/skills")),
    agent(
        "opencode",
        "OpenCode",
        ".agents/skills",
        config("opencode/skills"),
    ),
    agent(
        "openhands",
        "OpenHands",
        ".openhands/skills",
        home(".openhands/skills"),
    ),
    agent("ona", "Ona", ".ona/skills", home(".ona/skills")),
    agent("pi", "Pi", ".pi/skills", home(".pi/agent/skills")),
    agent("qoder", "Qoder", ".qoder/skills", home(".qoder/skills")),
    agent(
        "qoder-cn",
        "Qoder CN",
        ".qoder/skills",
        home(".qoder-cn/skills"),
    ),
    agent(
        "qwen-code",
        "Qwen Code",
        ".qwen/skills",
        home(".qwen/skills"),
    ),
    agent(
        "replit",
        "Replit",
        ".agents/skills",
        config("agents/skills"),
    ),
    agent(
        "reasonix",
        "Reasonix",
        ".reasonix/skills",
        home(".reasonix/skills"),
    ),
    agent(
        "rovodev",
        "Rovo Dev",
        ".rovodev/skills",
        home(".rovodev/skills"),
    ),
    agent("roo", "Roo Code", ".roo/skills", home(".roo/skills")),
    agent(
        "tabnine-cli",
        "Tabnine CLI",
        ".tabnine/agent/skills",
        home(".tabnine/agent/skills"),
    ),
    agent(
        "terramind",
        "Terramind",
        ".terramind/skills",
        home(".terramind/skills"),
    ),
    agent(
        "tinycloud",
        "Tinycloud",
        ".tinycloud/skills",
        home(".tinycloud/skills"),
    ),
    agent("trae", "Trae", ".trae/skills", home(".trae/skills")),
    agent(
        "trae-cn",
        "Trae CN",
        ".trae/skills",
        home(".trae-cn/skills"),
    ),
    agent("warp", "Warp", ".agents/skills", home(".agents/skills")),
    agent(
        "windsurf",
        "Windsurf",
        ".windsurf/skills",
        home(".codeium/windsurf/skills"),
    ),
    agent("zed", "Zed", ".agents/skills", home(".agents/skills")),
    agent("zcode", "ZCode", ".zcode/skills", home(".zcode/skills")),
    agent(
        "zencoder",
        "Zencoder",
        ".zencoder/skills",
        home(".zencoder/skills"),
    ),
    agent(
        "zenflow",
        "Zenflow",
        ".zencoder/skills",
        home(".zencoder/skills"),
    ),
    agent(
        "neovate",
        "Neovate",
        ".neovate/skills",
        home(".neovate/skills"),
    ),
    agent("pochi", "Pochi", ".pochi/skills", home(".pochi/skills")),
    agent(
        "promptscript",
        "PromptScript",
        ".agents/skills",
        GlobalLocation::None,
    ),
    agent("adal", "AdaL", ".adal/skills", home(".adal/skills")),
    agent(
        "universal",
        "Universal",
        ".agents/skills",
        config("agents/skills"),
    ),
];

// Older and agent-specific layouts still found in real projects. These are
// scanned in addition to the current agent registry.
pub(super) const LEGACY_LOCAL_ROOTS: &[&str] = &[
    ".cline/skills",
    ".codex/skills",
    ".cursor/skills",
    ".github/skills",
    ".opencode/skills",
];

const fn agent(
    key: &'static str,
    display_name: &'static str,
    local_skills_dir: &'static str,
    global_skills_dir: GlobalLocation,
) -> AgentSpec {
    AgentSpec {
        key,
        display_name,
        local_skills_dir,
        global_skills_dir,
    }
}

const fn home(path: &'static str) -> GlobalLocation {
    GlobalLocation::Home(path)
}

const fn config(path: &'static str) -> GlobalLocation {
    GlobalLocation::Config(path)
}

const fn env_home(
    variable: &'static str,
    fallback: &'static str,
    suffix: &'static str,
) -> GlobalLocation {
    GlobalLocation::EnvHome {
        variable,
        fallback,
        suffix,
    }
}
