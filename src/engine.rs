//! The colorization engine: turn a plain line into an ANSI-colored line by
//! running the compiled rules and painting the spans they claim with the active
//! theme's styles.
//!
//! Ownership model: rules run in order; the first rule to claim a byte owns it.
//! Later rules paint only still-unclaimed bytes. This lets specific structured
//! rules (a syslog prefix) take precedence over generic ones (bare numbers)
//! simply by being listed first — mirroring ccze's module-then-wordcolor flow.

use std::borrow::Cow;

use crate::color::Style;
use crate::rules::Rule;
use crate::theme::{tokens, Theme};

/// A compiled, ready-to-run colorizer: rules + the theme that styles their tokens.
#[derive(Debug, Clone)]
pub struct Colorizer {
    rules: Vec<Rule>,
    theme: Theme,
}

impl Colorizer {
    pub fn new(rules: Vec<Rule>, theme: Theme) -> Colorizer {
        Colorizer { rules, theme }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Swap the theme without recompiling rules (this is the "pick a theme" path).
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Colorize one line (without trailing newline). Returns a new `String` with
    /// ANSI escapes inserted. A line with no matches comes back styled with the
    /// theme's base/default color.
    pub fn colorize_line(&self, line: &str) -> String {
        // Logs piped in (e.g. `tail -F /var/log/**/*.log`) routinely carry
        // pre-existing terminal escapes and raw binary bytes. If we ran rules
        // over those bytes directly, a number/word rule would match the digits
        // *inside* an escape (`\x1b[31m`) and the renderer would inject its own
        // SGR codes mid-sequence — shredding the escape into visible garbage.
        // Stray control bytes likewise render as box/control glyphs. So strip
        // both before colorizing: no corruption, no binary chars on screen.
        let sanitized = sanitize(line);
        let line: &str = &sanitized;
        if line.is_empty() {
            return String::new();
        }
        let len = line.len();

        // owner[b] = index into `token_names` for the token owning byte b, or
        // usize::MAX for "unclaimed" (rendered with the default/base style).
        let mut owner = vec![usize::MAX; len];
        // Interned token names. All borrow from `self.rules`, so they share one lifetime.
        let mut token_names: Vec<&str> = Vec::new();

        for rule in &self.rules {
            if rule.has_named_groups {
                for caps in rule.regex.captures_iter(line) {
                    for (gi, tok) in rule.group_tokens.iter().enumerate() {
                        let Some(tok) = tok else { continue };
                        if let Some(m) = caps.get(gi) {
                            claim(&mut owner, &mut token_names, m.start(), m.end(), tok);
                        }
                    }
                }
            } else {
                for m in rule.regex.find_iter(line) {
                    claim(&mut owner, &mut token_names, m.start(), m.end(), &rule.whole_token);
                }
            }
        }

        self.render(line, &owner, &token_names)
    }

    /// Walk the byte-owner map, grouping maximal runs of equal token and emitting
    /// one styled chunk per run. Unclaimed runs get the base/default style.
    fn render(&self, line: &str, owner: &[usize], token_names: &[&str]) -> String {
        let default_style = self.theme.style(tokens::DEFAULT);
        let styles: Vec<Style> = token_names.iter().map(|n| self.theme.style(n)).collect();

        let mut out = String::with_capacity(line.len() + 16);
        let mut i = 0;
        let n = line.len();
        while i < n {
            let cur = owner[i];
            let mut j = i + 1;
            while j < n && owner[j] == cur {
                j += 1;
            }
            let style = if cur == usize::MAX { default_style } else { styles[cur] };
            // Slices land on char boundaries because all span ends come from regex
            // match offsets, which are always UTF-8 boundaries.
            out.push_str(&style.paint(&line[i..j]));
            i = j;
        }
        out
    }

    /// Colorize a whole multi-line string, preserving line breaks exactly.
    pub fn colorize_text(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() + text.len() / 8);
        for line in text.split_inclusive('\n') {
            if let Some(stripped) = line.strip_suffix('\n') {
                out.push_str(&self.colorize_line(stripped));
                out.push('\n');
            } else {
                out.push_str(&self.colorize_line(line));
            }
        }
        out
    }
}

/// Strip terminal escape sequences and stray control/binary bytes from a line.
///
/// This guarantees the colorizer never injects styles inside an existing escape
/// (which would corrupt it) and never emits raw control bytes that a terminal
/// renders as garbage glyphs. Tabs survive; every other C0 control, DEL, and the
/// C1 range (0x80–0x9F) is dropped, as are full ANSI/VT escape sequences. The
/// common case — a line with no control bytes — borrows the input unchanged.
fn sanitize(line: &str) -> Cow<'_, str> {
    let is_junk = |c: char| {
        let u = c as u32;
        (u < 0x20 && c != '\t') || u == 0x7f || (0x80..=0x9f).contains(&u)
    };
    if !line.chars().any(is_junk) {
        return Cow::Borrowed(line);
    }

    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // ESC — consume the whole escape sequence so nothing leaks through.
            '\u{1b}' => match chars.peek() {
                // CSI: ESC [ ... <final byte 0x40–0x7E>
                Some('[') => {
                    chars.next();
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... terminated by BEL or ST (ESC \)
                Some(']') => {
                    chars.next();
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c == '\u{07}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // Other escapes (ESC c, ESC ( B, …): drop ESC + the next byte.
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\t' => out.push('\t'),
            c if is_junk(c) => {} // C0/DEL/C1 control bytes: drop
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// Claim `[start, end)` for `token` on the owner map, but only the bytes that are
/// still unclaimed (earlier rules win). Interns the token name into `names`.
fn claim<'a>(
    owner: &mut [usize],
    names: &mut Vec<&'a str>,
    start: usize,
    end: usize,
    token: &'a str,
) {
    let id = match names.iter().position(|n| *n == token) {
        Some(i) => i,
        None => {
            names.push(token);
            names.len() - 1
        }
    };
    for b in &mut owner[start..end] {
        if *b == usize::MAX {
            *b = id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{builtin_generic, compile_all};
    use crate::theme::cyberpunk;

    fn engine() -> Colorizer {
        let rules = compile_all(&builtin_generic()).unwrap();
        Colorizer::new(rules, cyberpunk())
    }

    #[test]
    fn empty_line() {
        assert_eq!(engine().colorize_line(""), "");
    }

    #[test]
    fn plain_line_gets_base_style() {
        let out = engine().colorize_line("hello world");
        assert!(out.contains("\x1b["));
        assert!(out.contains("hello"));
    }

    #[test]
    fn error_word_is_painted_bold() {
        let out = engine().colorize_line("ERROR something broke");
        // The default palette styles ERROR bold; it must carry a bold SGR.
        assert!(out.contains("\x1b[1;"));
        assert!(out.contains("ERROR"));
    }

    #[test]
    fn newlines_preserved() {
        let out = engine().colorize_text("a\nb\n");
        assert_eq!(out.matches('\n').count(), 2);
    }

    #[test]
    fn strips_existing_escapes_no_corruption() {
        // Input already carries ANSI color. We must not tear it apart by
        // injecting our own SGR inside it: the foreign escapes are removed
        // entirely, leaving only our own clean, balanced sequences.
        let out = engine().colorize_line("ERROR \x1b[31mred\x1b[0m done");
        // Foreign escapes gone; the visible text survives.
        assert!(out.contains("red"));
        assert!(out.contains("done"));
        // Every escape we emit is well-formed: a `\x1b[` is always followed by
        // digits/semicolons and an `m` — never another `\x1b` (the corruption).
        assert!(!out.contains("\x1b[\x1b["));
        assert!(!out.contains("\x1b[m") || out.contains("\x1b[0m"));
        // No stray `[31m`/`[0m` literals leaked from the input.
        assert!(!out.contains("[31m"));
    }

    #[test]
    fn drops_binary_control_bytes() {
        // C0 controls, DEL, and C1 bytes must never reach the terminal.
        let out = engine().colorize_line("a\x00b\x07c\x7fd\u{0099}e");
        assert!(out.contains('a') && out.contains('e'));
        // Note: the output *does* contain `\x1b` — that's our own SGR styling.
        // What must never leak are the raw binary bytes from the input.
        for junk in ['\u{0}', '\u{7}', '\u{7f}', '\u{99}'] {
            assert!(!out.contains(junk), "control char {:#x} leaked", junk as u32);
        }
    }

    #[test]
    fn pure_control_line_is_empty() {
        // A line that is nothing but binary noise sanitizes to empty — no
        // dangling base-style escape, no garbage.
        assert_eq!(engine().colorize_line("\x1b[0m\x00\x07"), "");
    }

    #[test]
    fn tabs_preserved() {
        assert!(engine().colorize_line("a\tb").contains('\t'));
    }

    #[test]
    fn earlier_rule_wins_overlap() {
        // "192.168.0.1" should be one IP span (neon-sprawl ip = indexed 135),
        // not chopped by the number rule.
        let out = engine().colorize_line("192.168.0.1");
        assert!(out.contains("38;5;135"));
        // exactly one styled chunk -> exactly one reset.
        assert_eq!(out.matches("\x1b[0m").count(), 1);
    }

    #[test]
    fn multibyte_chars_survive_span_slicing() {
        // The owner map is byte-indexed; spans painted next to multibyte UTF-8 must
        // not split a char (which would panic) and the glyphs must come back intact.
        let out = engine().colorize_line("héllo wörld 42 ☃");
        assert!(out.contains("héllo"), "accented word preserved");
        assert!(out.contains("wörld"), "second accented word preserved");
        assert!(out.contains('☃'), "snowman survives");
        // The number rule still fired between multibyte runs.
        assert!(out.contains("42"));
        // Output is valid UTF-8 by construction (it's a String), so no boundary panic.
    }

    #[test]
    fn multibyte_directly_adjacent_to_match() {
        // A digit run flush against a multibyte char — the tightest boundary case.
        let out = engine().colorize_line("café42");
        assert!(out.contains("café"));
        assert!(out.contains("42"));
    }

    #[test]
    fn strips_osc_hyperlink_escape() {
        // OSC 8 hyperlink wrappers (ESC ] 8 ;; uri ST text ESC ] 8 ;; ST) must be
        // removed entirely, leaving the visible label.
        let out = engine().colorize_line("see \x1b]8;;https://x.com\x1b\\link\x1b]8;;\x1b\\ here");
        assert!(out.contains("link") && out.contains("here"));
        assert!(!out.contains("https://x.com"), "OSC payload stripped");
        assert!(!out.contains("8;;"), "no OSC fragment leaked");
    }

    #[test]
    fn strips_osc_terminated_by_bel() {
        // OSC title-set ending in BEL rather than ST.
        let out = engine().colorize_line("a\x1b]0;window title\x07b");
        assert!(out.contains('a') && out.contains('b'));
        assert!(!out.contains("window title"));
    }

    #[test]
    fn set_theme_recolors_without_recompiling() {
        let mut cz = engine();
        let before = cz.colorize_line("ERROR boom");
        cz.set_theme(crate::theme::ccze_classic());
        let after = cz.colorize_line("ERROR boom");
        assert_ne!(before, after, "swapping the theme changes the output");
        assert!(after.contains("ERROR"));
    }

    #[test]
    fn colorize_text_without_trailing_newline() {
        // The final line has no '\n'; it must still be colorized and not get one appended.
        let out = engine().colorize_text("first\nsecond");
        assert_eq!(out.matches('\n').count(), 1);
        assert!(out.contains("first") && out.contains("second"));
        assert!(!out.ends_with('\n'));
    }

    #[test]
    fn colorize_text_preserves_blank_lines() {
        // A blank middle line keeps its newline; an empty line colorizes to "".
        let out = engine().colorize_text("a\n\nb\n");
        assert_eq!(out.matches('\n').count(), 3);
    }

    #[test]
    fn default_cyberpunk_constructor_works() {
        let cz = Colorizer::default_cyberpunk();
        assert_eq!(cz.theme().name, crate::theme::DEFAULT_THEME);
        assert!(!cz.rules().is_empty());
    }

    #[test]
    fn whitespace_only_line_keeps_its_spaces() {
        // Spaces aren't junk, so a run of them passes through (in the base style).
        let out = engine().colorize_line("   ");
        assert!(out.contains("   "));
    }

    #[test]
    fn later_rule_paints_only_unclaimed_tail() {
        // First rule claims "abc"; the overlapping second rule ("bcd") may only
        // paint the still-free trailing "d" — the heart of the ownership model.
        let defs = vec![
            crate::rules::RuleDef::with_token("first", "abc", "error"),
            crate::rules::RuleDef::with_token("second", "bcd", "info"),
        ];
        let cz = Colorizer::new(compile_all(&defs).unwrap(), cyberpunk());
        let out = cz.colorize_line("abcd");
        let err = cz.theme().style("error");
        let info = cz.theme().style("info");
        assert!(out.contains(&err.paint("abc")), "first rule owns abc");
        assert!(out.contains(&info.paint("d")), "second rule paints only the tail d");
    }

    #[test]
    fn esc_two_byte_escape_dropped() {
        // ESC ( (charset designation, ESC + one byte) is removed whole; the rest survives.
        let out = engine().colorize_line("X\x1b(Y");
        assert!(out.contains('X') && out.contains('Y'));
    }

    #[test]
    fn lone_trailing_esc_is_dropped() {
        // A bare ESC at end-of-line (no following byte) must drop cleanly, no panic.
        let out = engine().colorize_line("done\x1b");
        assert!(out.contains("done"));
    }
}
