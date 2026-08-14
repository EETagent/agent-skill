use agent_skill::terminal::{sanitize_inline, sanitize_multiline};
use pretty_assertions::assert_eq;

#[test]
fn strips_csi_osc_and_dcs_sequences() {
    let input =
        "plain\u{001b}[31mred\u{001b}[0m\u{001b}]0;title\u{0007}done\u{001b}Ppayload\u{001b}\\end";
    assert_eq!(sanitize_inline(input), "plainreddoneend");
}

#[test]
fn handles_c1_string_terminator_and_line_endings() {
    let input = "a\u{009d}hidden\u{009c}b\r\nc\rd\te";
    assert_eq!(sanitize_multiline(input), "ab\nc\nd    e");
    assert_eq!(sanitize_inline(input), "ab c d e");
}

#[test]
fn strips_all_c1_control_string_families() {
    let input = concat!(
        "a\u{0098}sos\u{009c}",
        "b\u{009e}privacy-message\u{009c}",
        "c\u{009f}application-program-command\u{009c}d"
    );

    assert_eq!(sanitize_inline(input), "abcd");
}

#[test]
fn consumes_complete_escape_sequences_with_intermediate_bytes() {
    let input = "before\u{001b}#8after\u{001b}(Bdone";

    assert_eq!(sanitize_inline(input), "beforeafterdone");
}

#[test]
fn bell_only_terminates_osc_control_strings() {
    let input = concat!(
        "a\u{001b}Pprivate\u{0007}still-private\u{001b}\\b",
        "c\u{001b}]title\u{0007}d"
    );

    assert_eq!(sanitize_inline(input), "abcd");
}
