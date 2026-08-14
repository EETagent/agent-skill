use std::{iter::Peekable, str::Chars};

/// Removes terminal control sequences and folds all whitespace onto one line.
pub fn sanitize_inline(value: &str) -> String {
    sanitize(value, WhitespaceMode::Inline)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Removes terminal control sequences while preserving readable line breaks.
pub fn sanitize_multiline(value: &str) -> String {
    sanitize(value, WhitespaceMode::Multiline)
}

#[derive(Debug, Clone, Copy)]
enum WhitespaceMode {
    Inline,
    Multiline,
}

#[derive(Debug, Clone, Copy)]
enum ControlString {
    Osc,
    Other,
}

fn sanitize(value: &str, mode: WhitespaceMode) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\u{001b}' => consume_escape_sequence(&mut characters),
            '\u{009d}' => consume_control_string(&mut characters, ControlString::Osc),
            '\u{0090}' | '\u{0098}' | '\u{009e}' | '\u{009f}' => {
                consume_control_string(&mut characters, ControlString::Other)
            }
            '\u{009b}' => consume_csi(&mut characters),
            '\u{009c}' => {}
            '\n' => match mode {
                WhitespaceMode::Inline => output.push(' '),
                WhitespaceMode::Multiline => output.push('\n'),
            },
            '\r' => match mode {
                WhitespaceMode::Inline => output.push(' '),
                WhitespaceMode::Multiline if characters.peek() != Some(&'\n') => {
                    output.push('\n');
                }
                WhitespaceMode::Multiline => {}
            },
            '\t' => match mode {
                WhitespaceMode::Inline => output.push(' '),
                WhitespaceMode::Multiline => output.push_str("    "),
            },
            character if character.is_control() => {}
            _ => output.push(character),
        }
    }

    output
}

fn consume_escape_sequence(characters: &mut Peekable<Chars<'_>>) {
    let Some(introducer) = characters.next() else {
        return;
    };

    match introducer {
        '[' => consume_csi(characters),
        ']' => consume_control_string(characters, ControlString::Osc),
        'P' | 'X' | '^' | '_' => consume_control_string(characters, ControlString::Other),
        '\u{0020}'..='\u{002f}' => consume_escape_intermediates(characters),
        _ => {
            // ESC followed by a final byte is a complete two-byte sequence.
        }
    }
}

fn consume_escape_intermediates(characters: &mut Peekable<Chars<'_>>) {
    while characters
        .peek()
        .is_some_and(|character| ('\u{0020}'..='\u{002f}').contains(character))
    {
        characters.next();
    }

    if characters
        .peek()
        .is_some_and(|character| ('\u{0030}'..='\u{007e}').contains(character))
    {
        characters.next();
    }
}

fn consume_csi(characters: &mut Peekable<Chars<'_>>) {
    for character in characters.by_ref() {
        if ('@'..='~').contains(&character) {
            break;
        }
    }
}

fn consume_control_string(characters: &mut Peekable<Chars<'_>>, kind: ControlString) {
    while let Some(character) = characters.next() {
        if character == '\u{009c}' {
            break;
        }
        if matches!(kind, ControlString::Osc) && character == '\u{0007}' {
            break;
        }
        if character == '\u{001b}' && characters.peek() == Some(&'\\') {
            characters.next();
            break;
        }
    }
}
