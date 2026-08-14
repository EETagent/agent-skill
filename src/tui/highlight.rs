use std::{ops::Range, sync::LazyLock};

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, Style as SyntectStyle, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};
use unicode_width::UnicodeWidthChar;

const MARKDOWN_EXTENSION: &str = "md";
const THEME_NAME: &str = "base16-ocean.dark";

struct HighlightAssets {
    syntaxes: SyntaxSet,
    theme: Theme,
}

static HIGHLIGHT_ASSETS: LazyLock<HighlightAssets> = LazyLock::new(|| {
    let themes = ThemeSet::load_defaults();
    HighlightAssets {
        syntaxes: SyntaxSet::load_defaults_newlines(),
        theme: themes.themes[THEME_NAME].clone(),
    }
});

#[derive(Debug, Clone, PartialEq, Eq)]
struct StyledRegion {
    range: Range<usize>,
    style: Style,
}

/// Highlights Markdown without transforming it, then wraps it into owned terminal rows.
/// Any Syntect failure deliberately falls back to unstyled source.
pub(super) fn markdown_lines(source: &str, width: usize) -> Vec<Line<'static>> {
    highlighted_or_plain_lines(source, width, markdown_regions(source))
}

fn highlighted_or_plain_lines(
    source: &str,
    width: usize,
    regions: Option<Vec<StyledRegion>>,
) -> Vec<Line<'static>> {
    let regions = regions.unwrap_or_else(|| unstyled_regions(source));
    wrap_regions(source, &regions, width)
}

/// Wraps generated UI text while retaining every span's style across row boundaries.
pub(super) fn wrap_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    let mut builder = WrappedLines::new(width);
    for span in spans {
        builder.push(&span.content, span.style);
    }
    builder.finish()
}

fn markdown_regions(source: &str) -> Option<Vec<StyledRegion>> {
    if source.is_empty() {
        return Some(Vec::new());
    }

    let assets = &*HIGHLIGHT_ASSETS;
    let markdown = assets
        .syntaxes
        .find_syntax_by_extension(MARKDOWN_EXTENSION)?;
    let base = highlight_with_syntax(source, markdown, assets)?;
    let overlays = fenced_code_regions(source, assets)?;
    compose_regions(source.len(), &base, &overlays)
}

fn fenced_code_regions(source: &str, assets: &HighlightAssets) -> Option<Vec<StyledRegion>> {
    let mut overlays = Vec::new();

    for (event, offsets) in Parser::new(source).into_offset_iter() {
        let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event else {
            continue;
        };
        let Some(language) = info
            .split_whitespace()
            .next()
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some(syntax) = assets.syntaxes.find_syntax_by_token(language) else {
            continue;
        };
        let Some(body) = complete_fenced_body(source, offsets.start) else {
            continue;
        };

        let relative_regions = highlight_with_syntax(&source[body.clone()], syntax, assets)?;
        overlays.extend(relative_regions.into_iter().map(|region| StyledRegion {
            range: body.start + region.range.start..body.start + region.range.end,
            style: region.style,
        }));
    }

    overlays.sort_by_key(|region| region.range.start);
    if overlays
        .windows(2)
        .any(|regions| regions[0].range.end > regions[1].range.start)
    {
        return None;
    }
    Some(overlays)
}

fn complete_fenced_body(source: &str, block_start: usize) -> Option<Range<usize>> {
    let line_start = source[..block_start.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let opening_end = source[line_start..]
        .find('\n')
        .map(|index| line_start + index)?;
    let (marker, marker_length) = opening_fence(&source[line_start..opening_end])?;
    let body_start = opening_end + 1;
    let mut cursor = body_start;

    loop {
        let line_end = source[cursor..]
            .find('\n')
            .map_or(source.len(), |index| cursor + index);
        if closing_fence(&source[cursor..line_end], marker, marker_length) {
            return Some(body_start..cursor);
        }
        if line_end == source.len() {
            return None;
        }
        cursor = line_end + 1;
    }
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    let content = strip_fence_indent(line)?;
    let marker = content
        .chars()
        .next()
        .filter(|marker| matches!(marker, '`' | '~'))?;
    let marker_length = content
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (marker_length >= 3).then_some((marker, marker_length))
}

fn closing_fence(line: &str, marker: char, minimum_length: usize) -> bool {
    let Some(content) = strip_fence_indent(line) else {
        return false;
    };
    let marker_length = content
        .chars()
        .take_while(|character| *character == marker)
        .count();
    marker_length >= minimum_length && content[marker_length..].chars().all(char::is_whitespace)
}

fn strip_fence_indent(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    (indent <= 3).then(|| &line[indent..])
}

fn highlight_with_syntax(
    source: &str,
    syntax: &SyntaxReference,
    assets: &HighlightAssets,
) -> Option<Vec<StyledRegion>> {
    if source.is_empty() {
        return Some(Vec::new());
    }

    let mut highlighter = HighlightLines::new(syntax, &assets.theme);
    let mut regions = Vec::new();
    let mut offset = 0usize;

    for line in LinesWithEndings::from(source) {
        for (style, text) in highlighter.highlight_line(line, &assets.syntaxes).ok()? {
            let end = offset.checked_add(text.len())?;
            if source.get(offset..end)? != text {
                return None;
            }
            push_region(&mut regions, offset..end, syntect_style(style));
            offset = end;
        }
    }

    (offset == source.len()).then_some(regions)
}

fn compose_regions(
    source_length: usize,
    base: &[StyledRegion],
    overlays: &[StyledRegion],
) -> Option<Vec<StyledRegion>> {
    if source_length == 0 {
        return Some(Vec::new());
    }

    let mut boundaries = Vec::with_capacity((base.len() + overlays.len()) * 2 + 2);
    boundaries.extend([0, source_length]);
    for region in base.iter().chain(overlays) {
        if region.range.start > region.range.end || region.range.end > source_length {
            return None;
        }
        boundaries.extend([region.range.start, region.range.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut base_index = 0usize;
    let mut overlay_index = 0usize;
    let mut composed = Vec::new();
    for boundary in boundaries.windows(2) {
        let range = boundary[0]..boundary[1];
        if range.is_empty() {
            continue;
        }
        let overlay = region_at(overlays, &mut overlay_index, range.start);
        let style = overlay
            .or_else(|| region_at(base, &mut base_index, range.start))
            .map(|region| region.style)?;
        push_region(&mut composed, range, style);
    }
    Some(composed)
}

fn region_at<'a>(
    regions: &'a [StyledRegion],
    index: &mut usize,
    offset: usize,
) -> Option<&'a StyledRegion> {
    while regions
        .get(*index)
        .is_some_and(|region| region.range.end <= offset)
    {
        *index += 1;
    }
    regions
        .get(*index)
        .filter(|region| region.range.start <= offset && offset < region.range.end)
}

fn push_region(output: &mut Vec<StyledRegion>, range: Range<usize>, style: Style) {
    if range.is_empty() {
        return;
    }
    if let Some(previous) = output.last_mut() {
        if previous.range.end == range.start && previous.style == style {
            previous.range.end = range.end;
            return;
        }
    }
    output.push(StyledRegion { range, style });
}

fn unstyled_regions(source: &str) -> Vec<StyledRegion> {
    (!source.is_empty())
        .then(|| StyledRegion {
            range: 0..source.len(),
            style: Style::default(),
        })
        .into_iter()
        .collect()
}

fn syntect_style(style: SyntectStyle) -> Style {
    let mut output = Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ));
    if style.font_style.contains(FontStyle::BOLD) {
        output = output.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        output = output.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        output = output.add_modifier(Modifier::UNDERLINED);
    }
    output
}

fn wrap_regions(source: &str, regions: &[StyledRegion], width: usize) -> Vec<Line<'static>> {
    let mut builder = WrappedLines::new(width);
    for region in regions {
        let Some(text) = source.get(region.range.clone()) else {
            return wrap_spans(vec![Span::raw(source.to_owned())], width);
        };
        builder.push(text, region.style);
    }
    builder.finish()
}

struct WrappedLines {
    width: usize,
    rows: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    span_text: String,
    span_style: Option<Style>,
    row_width: usize,
    row_has_content: bool,
}

impl WrappedLines {
    fn new(width: usize) -> Self {
        Self {
            width: width.max(1),
            rows: Vec::new(),
            spans: Vec::new(),
            span_text: String::new(),
            span_style: None,
            row_width: 0,
            row_has_content: false,
        }
    }

    fn push(&mut self, text: &str, style: Style) {
        for character in text.chars() {
            if character == '\n' {
                self.finish_row();
                continue;
            }

            let character_width = UnicodeWidthChar::width(character).unwrap_or_default();
            if self.row_has_content && self.row_width.saturating_add(character_width) > self.width {
                self.finish_row();
            }
            if self.span_style != Some(style) {
                self.finish_span();
                self.span_style = Some(style);
            }
            self.span_text.push(character);
            self.row_width = self.row_width.saturating_add(character_width);
            self.row_has_content = true;
        }
    }

    fn finish_span(&mut self) {
        if self.span_text.is_empty() {
            return;
        }
        self.spans.push(Span::styled(
            std::mem::take(&mut self.span_text),
            self.span_style.unwrap_or_default(),
        ));
    }

    fn finish_row(&mut self) {
        self.finish_span();
        self.rows.push(Line::from(std::mem::take(&mut self.spans)));
        self.span_style = None;
        self.row_width = 0;
        self.row_has_content = false;
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.finish_row();
        self.rows
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Modifier, Style};
    use syntect::highlighting::{Color as SyntectColor, FontStyle, Style as SyntectStyle};

    use super::{
        fenced_code_regions, highlighted_or_plain_lines, markdown_regions, syntect_style,
        wrap_spans, HighlightAssets, HIGHLIGHT_ASSETS,
    };

    fn flattened_regions(source: &str) -> String {
        let regions = markdown_regions(source).expect("default Markdown highlighting should work");
        regions
            .iter()
            .map(|region| &source[region.range.clone()])
            .collect()
    }

    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn highlighting_preserves_markdown_source_exactly() {
        let source = "---\nname: preview\ndescription: Test\n---\n# Heading\n\n*emphasis* and [link](https://example.com/a?b=c) with `inline()`\n\n```rust\nfn main() { println!(\"hello\"); }\n```\n\n~~~bash\nprintf '%s\\n' \"hello\"\n~~~\n\n```unknown\nraw <markers> stay\n```\n\n```rust\nunclosed()\n";

        assert_eq!(flattened_regions(source), source);
    }

    #[test]
    fn overlays_only_recognized_complete_fenced_blocks() {
        let assets: &HighlightAssets = &HIGHLIGHT_ASSETS;
        let recognized = "```rust\nfn main() {}\n```\n~~~bash\necho ok\n~~~\n";
        let recognized_regions = fenced_code_regions(recognized, assets).unwrap();
        assert!(!recognized_regions.is_empty());
        let recognized_text: String = recognized_regions
            .iter()
            .map(|region| &recognized[region.range.clone()])
            .collect();
        assert!(recognized_text.contains("fn main"));
        assert!(recognized_text.contains("echo ok"));
        assert!(!recognized_text.contains("```"));
        assert!(!recognized_text.contains("~~~"));

        assert!(fenced_code_regions("```unknown\nvalue\n```\n", assets)
            .unwrap()
            .is_empty());
        assert!(fenced_code_regions("```rust\nfn value() {}\n", assets)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn converts_rgb_and_modifiers_without_a_background() {
        let converted = syntect_style(SyntectStyle {
            foreground: SyntectColor {
                r: 12,
                g: 34,
                b: 56,
                a: 255,
            },
            background: SyntectColor {
                r: 65,
                g: 43,
                b: 21,
                a: 255,
            },
            font_style: FontStyle::BOLD | FontStyle::ITALIC | FontStyle::UNDERLINE,
        });

        assert_eq!(converted.fg, Some(Color::Rgb(12, 34, 56)));
        assert_eq!(converted.bg, None);
        assert!(converted.add_modifier.contains(Modifier::BOLD));
        assert!(converted.add_modifier.contains(Modifier::ITALIC));
        assert!(converted.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn unicode_wrapping_preserves_span_styles() {
        let style = Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD);
        let rows = wrap_spans(vec![ratatui::text::Span::styled("ab界cd", style)], 3);

        assert_eq!(
            rows.iter().map(line_text).collect::<Vec<_>>(),
            ["ab", "界c", "d"]
        );
        assert!(rows
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.style == style));
    }

    #[test]
    fn highlighted_rows_do_not_introduce_control_sequences() {
        let source = "# Safe\n\n```rust\nlet value = 1;\n```\n";
        let rows = super::markdown_lines(source, 120);
        let flattened = rows.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert_eq!(flattened, source);
        assert!(!flattened.contains('\u{1b}'));
    }

    #[test]
    fn highlighting_failure_falls_back_to_unstyled_source() {
        let source = "# Still visible\nwith [source](https://example.com)\n";
        let rows = highlighted_or_plain_lines(source, 120, None);
        let flattened = rows.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert_eq!(flattened, source);
        assert!(rows
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.style == Style::default()));
    }
}
