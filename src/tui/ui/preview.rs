use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::tui::highlight::{markdown_lines, wrap_spans};
use crate::{
    model::{InstallableSkill, Skill, SkillState},
    paths::{shorten_path, Environment},
    terminal::{sanitize_inline, sanitize_multiline},
};

use super::INSTALL_PREVIEW_BYTES;

pub(super) fn build_skill_preview(
    skill: &Skill,
    environment: &Environment,
    width: usize,
) -> Vec<Line<'static>> {
    let mut preview = PreviewBuilder::new(width);
    preview.metadata("Name", &skill.name, Style::default());
    preview.metadata(
        "State",
        skill.state.label(),
        match skill.state {
            SkillState::Enabled => Style::default().fg(Color::Green),
            SkillState::Disabled => Style::default().fg(Color::Yellow),
        },
    );
    preview.metadata("Scope", skill.scope.label(), Style::default());
    preview.metadata(
        "Path",
        &shorten_path(&skill.path, environment),
        Style::default(),
    );
    preview.metadata("Agents", &skill.agents.join(", "), Style::default());
    preview.optional_metadata("Description", &skill.description, Style::default());
    if !skill.warnings.is_empty() {
        preview.metadata(
            "Warnings",
            &skill.warnings.join("; "),
            Style::default().fg(Color::Yellow),
        );
    }
    preview.source(&sanitize_multiline(&skill.content));
    preview.finish()
}

pub(super) fn build_install_preview(skill: &InstallableSkill, width: usize) -> Vec<Line<'static>> {
    let mut preview = PreviewBuilder::new(width);
    preview.metadata("Title", &skill.title, Style::default());
    preview.metadata("Name", &skill.name, Style::default());
    preview.metadata(
        "Path",
        &skill.relative_path.display().to_string(),
        Style::default(),
    );
    preview.optional_metadata("Description", &skill.description, Style::default());
    if !skill.warnings.is_empty() {
        preview.metadata(
            "Warnings",
            &skill.warnings.join("; "),
            Style::default().fg(Color::Yellow),
        );
    }
    preview.source(&source_preview_excerpt(&skill.content));
    preview.finish()
}

struct PreviewBuilder {
    width: usize,
    lines: Vec<Line<'static>>,
}

impl PreviewBuilder {
    fn new(width: usize) -> Self {
        Self {
            width,
            lines: Vec::new(),
        }
    }

    fn metadata(&mut self, label: &str, value: &str, value_style: Style) {
        let value = sanitize_inline(value);
        let mut spans = vec![Span::styled(
            format!("{label}:"),
            Style::default().fg(Color::DarkGray),
        )];
        if !value.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(value, value_style));
        }
        self.lines.extend(wrap_spans(spans, self.width));
    }

    fn optional_metadata(&mut self, label: &str, value: &str, value_style: Style) {
        if !value.is_empty() {
            self.metadata(label, value, value_style);
        }
    }

    fn source(&mut self, content: &str) {
        self.lines.push(Line::default());
        self.lines.extend(markdown_lines(content, self.width));
    }

    fn finish(self) -> Vec<Line<'static>> {
        self.lines
    }
}

fn source_preview_excerpt(content: &str) -> String {
    let mut end = content.len().min(INSTALL_PREVIEW_BYTES);
    while !content.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }

    let mut excerpt = sanitize_multiline(&content[..end]);
    if end < content.len() {
        excerpt.push_str("\n\n[… source preview truncated …]");
    }
    excerpt
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{
        style::{Color, Style},
        text::Line,
    };

    use crate::{
        model::{InstallableSkill, Scope, Skill, SkillMetadata, SkillState},
        paths::Environment,
        terminal::sanitize_multiline,
    };

    use super::{
        build_install_preview, build_skill_preview, source_preview_excerpt, INSTALL_PREVIEW_BYTES,
    };

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn source_rows(lines: &[Line<'_>]) -> String {
        let separator = lines
            .iter()
            .position(|line| line_text(line).is_empty())
            .expect("preview metadata should be separated from source");
        lines[separator + 1..]
            .iter()
            .map(line_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn value_style(lines: &[Line<'_>], label: &str, value: &str) -> Style {
        let line = lines
            .iter()
            .find(|line| line_text(line).starts_with(label))
            .expect("metadata line should exist");
        line.spans
            .iter()
            .find(|span| span.content == value)
            .expect("metadata value should have its own span")
            .style
    }

    #[test]
    fn main_preview_sanitizes_then_highlights_source_and_styles_metadata() {
        let content = "---\nname: demo\n---\n# H\u{1b}[31meading\n\n```rust\nlet value = 1;\n```\n";
        let mut skill = Skill {
            id: "skill-one".to_owned(),
            name: "demo".to_owned(),
            title: "Demo".to_owned(),
            description: "A preview".to_owned(),
            path: PathBuf::from("/project/.agents/skills/demo"),
            relative_path: PathBuf::from("demo"),
            root_id: "root".to_owned(),
            root_path: PathBuf::from("/project/.agents/skills"),
            scope: Scope::Local,
            state: SkillState::Enabled,
            agents: vec!["Codex".to_owned()],
            content: content.to_owned(),
            warnings: vec!["preview warning".to_owned()],
        };
        let environment = Environment {
            project_root: PathBuf::from("/project"),
            home_dir: PathBuf::from("/home/test"),
            config_home: PathBuf::from("/home/test/.config"),
            native_config_home: None,
        };

        let lines = build_skill_preview(&skill, &environment, 200);

        assert_eq!(source_rows(&lines), sanitize_multiline(content));
        assert!(!source_rows(&lines).contains('\u{1b}'));
        assert_eq!(
            value_style(&lines, "State:", "enabled").fg,
            Some(Color::Green)
        );
        assert_eq!(
            value_style(&lines, "Warnings:", "preview warning").fg,
            Some(Color::Yellow)
        );
        assert!(lines
            .iter()
            .skip_while(|line| !line_text(line).starts_with("---"))
            .flat_map(|line| &line.spans)
            .any(|span| span.style != Style::default()));

        skill.state = SkillState::Disabled;
        let disabled_lines = build_skill_preview(&skill, &environment, 200);
        assert_eq!(
            value_style(&disabled_lines, "State:", "disabled").fg,
            Some(Color::Yellow)
        );
    }

    #[test]
    fn install_preview_sanitizes_then_highlights_source_and_styles_warnings() {
        let content = "# Install \u{1b}[2Jpreview\n\n```bash\necho ready\n```\n";
        let skill = InstallableSkill::new(
            SkillMetadata {
                name: "install-demo".to_owned(),
                title: "Install demo".to_owned(),
                description: "Source description".to_owned(),
                content: content.to_owned(),
                warnings: vec!["source warning".to_owned()],
            },
            PathBuf::from("/source/install-demo"),
            PathBuf::from("skills/install-demo"),
        );

        let lines = build_install_preview(&skill, 200);

        assert_eq!(source_rows(&lines), sanitize_multiline(content));
        assert!(!source_rows(&lines).contains('\u{1b}'));
        assert_eq!(
            value_style(&lines, "Warnings:", "source warning").fg,
            Some(Color::Yellow)
        );
        assert!(lines
            .iter()
            .skip_while(|line| !line_text(line).starts_with("# Install"))
            .flat_map(|line| &line.spans)
            .any(|span| span.style != Style::default()));
    }

    #[test]
    fn install_excerpt_preserves_utf8_boundary_and_limit() {
        let content = "é".repeat(INSTALL_PREVIEW_BYTES);
        let excerpt = source_preview_excerpt(&content);

        assert!(excerpt.is_char_boundary(excerpt.len()));
        assert!(excerpt.starts_with(&content[..INSTALL_PREVIEW_BYTES]));
        assert!(excerpt.ends_with("[… source preview truncated …]"));
    }
}
