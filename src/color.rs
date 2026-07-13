//! Color and text-style model plus ANSI SGR rendering.
//!
//! A [`Color`] can be one of the 16 named terminal colors, a 256-color palette
//! index, or a 24-bit truecolor RGB triple. A [`Style`] bundles a foreground
//! color with optional background and attributes (bold/underline/…). Styles are
//! what a [`crate::theme::Theme`] assigns to each semantic token.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// A terminal color in one of three representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    /// One of the 16 named ANSI colors (`"red"`, `"bright_cyan"`, …).
    Named(Named),
    /// 256-color palette index, written in config as `{ index = 213 }`.
    Indexed(Indexed),
    /// 24-bit truecolor, written in config as `"#ff00aa"` or `{ rgb = [255, 0, 170] }`.
    Rgb(Rgb),
}

/// The 16 standard ANSI color names (normal + bright).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Named {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Named {
    /// SGR foreground code base. Normal colors are 30–37, bright are 90–97.
    fn fg_code(self) -> u8 {
        use Named::*;
        match self {
            Black => 30,
            Red => 31,
            Green => 32,
            Yellow => 33,
            Blue => 34,
            Magenta => 35,
            Cyan => 36,
            White => 37,
            BrightBlack => 90,
            BrightRed => 91,
            BrightGreen => 92,
            BrightYellow => 93,
            BrightBlue => 94,
            BrightMagenta => 95,
            BrightCyan => 96,
            BrightWhite => 97,
        }
    }
}

/// 256-color palette index wrapper (so it deserializes from `{ index = N }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Indexed {
    pub index: u8,
}

/// 24-bit RGB triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RgbRepr", into = "RgbRepr")]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// On-disk forms for [`Rgb`]: either a `"#rrggbb"` string or `{ rgb = [r,g,b] }`.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RgbRepr {
    Hex(String),
    Triple { rgb: [u8; 3] },
}

impl From<RgbRepr> for Rgb {
    fn from(r: RgbRepr) -> Self {
        match r {
            RgbRepr::Hex(s) => Rgb::from_hex(&s).unwrap_or(Rgb { r: 0, g: 0, b: 0 }),
            RgbRepr::Triple { rgb } => Rgb {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            },
        }
    }
}

impl From<Rgb> for RgbRepr {
    fn from(c: Rgb) -> Self {
        RgbRepr::Hex(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
    }
}

impl Rgb {
    /// Parse `#rgb` or `#rrggbb` (leading `#` optional).
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let s = s.trim().trim_start_matches('#');
        let parse = |h: &str| u8::from_str_radix(h, 16).ok();
        match s.len() {
            3 => {
                let dup = |c: char| {
                    let d = c.to_digit(16)? as u8;
                    Some(d * 16 + d)
                };
                let mut it = s.chars();
                Some(Rgb {
                    r: dup(it.next()?)?,
                    g: dup(it.next()?)?,
                    b: dup(it.next()?)?,
                })
            }
            6 => Some(Rgb {
                r: parse(&s[0..2])?,
                g: parse(&s[2..4])?,
                b: parse(&s[4..6])?,
            }),
            _ => None,
        }
    }
}

impl Color {
    /// Convenience: build a truecolor from a `#rrggbb` literal. Panics on bad input
    /// (intended for `const`-ish builtin theme tables where the literal is trusted).
    pub fn hex(s: &str) -> Color {
        Color::Rgb(Rgb::from_hex(s).expect("valid hex color literal"))
    }

    /// Convenience: a 256-color palette index (used by the ported iftoprs palettes).
    pub fn idx(i: u8) -> Color {
        Color::Indexed(Indexed { index: i })
    }

    /// Append the SGR parameters that set this color as *foreground* (no `\x1b[` / `m`).
    fn write_fg(self, out: &mut String) {
        match self {
            Color::Named(n) => {
                let _ = write!(out, "{}", n.fg_code());
            }
            Color::Indexed(i) => {
                let _ = write!(out, "38;5;{}", i.index);
            }
            Color::Rgb(c) => {
                let _ = write!(out, "38;2;{};{};{}", c.r, c.g, c.b);
            }
        }
    }

    /// Append the SGR parameters that set this color as *background*.
    fn write_bg(self, out: &mut String) {
        match self {
            Color::Named(n) => {
                // Background named codes are foreground + 10.
                let _ = write!(out, "{}", n.fg_code() + 10);
            }
            Color::Indexed(i) => {
                let _ = write!(out, "48;5;{}", i.index);
            }
            Color::Rgb(c) => {
                let _ = write!(out, "48;2;{};{};{}", c.r, c.g, c.b);
            }
        }
    }
}

/// A fully-resolved text style: foreground color plus optional background and attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
}

impl Style {
    pub const RESET: &'static str = "\x1b[0m";

    /// A style that applies no color and no attributes.
    pub fn plain() -> Style {
        Style::default()
    }

    pub fn fg(color: Color) -> Style {
        Style {
            fg: Some(color),
            ..Style::default()
        }
    }

    pub fn bold_fg(color: Color) -> Style {
        Style {
            fg: Some(color),
            bold: true,
            ..Style::default()
        }
    }

    /// True when this style would emit no escape at all.
    pub fn is_plain(&self) -> bool {
        *self == Style::default()
    }

    /// Modulate this style by a novelty *intensity* `t` in `[0, 1]` (see
    /// [`crate::novelty`]). Intensity drives the terminal attributes only — the
    /// colour is left untouched:
    ///
    /// * `t >= 0.9` — first-seen / anomalous: `bold` + `reverse` (blazes).
    /// * `t >= 0.4` — rare: `bold`.
    /// * `t <  0.15` — high-frequency noise: `dim`.
    /// * otherwise — unchanged.
    ///
    /// Thresholds are clamped, so an out-of-range `t` degrades gracefully.
    pub fn with_intensity(mut self, t: f32) -> Style {
        let t = t.clamp(0.0, 1.0);
        if t >= 0.9 {
            self.bold = true;
            self.reverse = true;
            self.dim = false;
        } else if t >= 0.4 {
            self.bold = true;
            self.dim = false;
        } else if t < 0.15 {
            self.dim = true;
            self.bold = false;
            self.reverse = false;
        }
        self
    }

    /// The opening SGR escape sequence for this style, e.g. `\x1b[1;38;2;255;0;170m`.
    /// Returns an empty string for a plain style.
    pub fn prefix(&self) -> String {
        if self.is_plain() {
            return String::new();
        }
        let mut params: Vec<String> = Vec::new();
        if self.bold {
            params.push("1".into());
        }
        if self.dim {
            params.push("2".into());
        }
        if self.italic {
            params.push("3".into());
        }
        if self.underline {
            params.push("4".into());
        }
        if self.blink {
            params.push("5".into());
        }
        if self.reverse {
            params.push("7".into());
        }
        if let Some(fg) = self.fg {
            let mut s = String::new();
            fg.write_fg(&mut s);
            params.push(s);
        }
        if let Some(bg) = self.bg {
            let mut s = String::new();
            bg.write_bg(&mut s);
            params.push(s);
        }
        format!("\x1b[{}m", params.join(";"))
    }

    /// Wrap `text` in this style's escape + a reset. No-op (returns `text` owned) when plain.
    pub fn paint(&self, text: &str) -> String {
        if self.is_plain() {
            return text.to_string();
        }
        format!("{}{}{}", self.prefix(), text, Style::RESET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        assert_eq!(
            Rgb::from_hex("#ff00aa"),
            Some(Rgb {
                r: 255,
                g: 0,
                b: 170
            })
        );
        assert_eq!(Rgb::from_hex("0a0"), Some(Rgb { r: 0, g: 170, b: 0 }));
        assert_eq!(Rgb::from_hex("nope"), None);
    }

    #[test]
    fn truecolor_prefix() {
        let s = Style::bold_fg(Color::hex("#ff00aa"));
        assert_eq!(s.prefix(), "\x1b[1;38;2;255;0;170m");
    }

    #[test]
    fn named_bg_offset() {
        let s = Style {
            bg: Some(Color::Named(Named::Blue)),
            ..Style::default()
        };
        assert_eq!(s.prefix(), "\x1b[44m");
    }

    #[test]
    fn hex_parsing_edge_cases() {
        // Uppercase digits parse the same as lowercase.
        assert_eq!(
            Rgb::from_hex("#FF00AA"),
            Some(Rgb {
                r: 255,
                g: 0,
                b: 170
            })
        );
        // Leading/trailing whitespace is tolerated.
        assert_eq!(Rgb::from_hex("  #0a0  "), Some(Rgb { r: 0, g: 170, b: 0 }));
        // Wrong lengths and empty input are rejected.
        assert_eq!(Rgb::from_hex(""), None);
        assert_eq!(Rgb::from_hex("#12345"), None);
        assert_eq!(Rgb::from_hex("#1234567"), None);
        // A non-hex digit in an otherwise well-sized string fails.
        assert_eq!(Rgb::from_hex("#gg0000"), None);
    }

    #[test]
    fn indexed_fg_and_bg_prefix() {
        // 256-color foreground uses the 38;5;N form, background 48;5;N.
        assert_eq!(Style::fg(Color::idx(196)).prefix(), "\x1b[38;5;196m");
        let bg = Style {
            bg: Some(Color::idx(21)),
            ..Style::default()
        };
        assert_eq!(bg.prefix(), "\x1b[48;5;21m");
    }

    #[test]
    fn truecolor_bg_prefix() {
        let s = Style {
            bg: Some(Color::hex("#102030")),
            ..Style::default()
        };
        assert_eq!(s.prefix(), "\x1b[48;2;16;32;48m");
    }

    #[test]
    fn all_attributes_emit_in_canonical_order() {
        // Every attribute set at once → 1;2;3;4;5;7 (bold,dim,italic,underline,blink,reverse).
        let s = Style {
            bold: true,
            dim: true,
            italic: true,
            underline: true,
            blink: true,
            reverse: true,
            ..Style::default()
        };
        assert_eq!(s.prefix(), "\x1b[1;2;3;4;5;7m");
    }

    #[test]
    fn attributes_precede_colors() {
        // Bold (an attribute) comes before the fg color params.
        let s = Style::bold_fg(Color::idx(9));
        assert_eq!(s.prefix(), "\x1b[1;38;5;9m");
    }

    #[test]
    fn plain_style_emits_nothing_and_paint_is_identity() {
        let p = Style::plain();
        assert!(p.is_plain());
        assert_eq!(p.prefix(), "");
        // paint() on a plain style returns the text verbatim — no escapes at all.
        assert_eq!(p.paint("hello"), "hello");
    }

    #[test]
    fn paint_wraps_with_prefix_and_reset() {
        let s = Style::fg(Color::Named(Named::Red));
        assert_eq!(s.paint("x"), "\x1b[31mx\x1b[0m");
    }

    #[test]
    fn idx_and_hex_constructors() {
        assert_eq!(Color::idx(213), Color::Indexed(Indexed { index: 213 }));
        assert_eq!(
            Color::hex("#ff00aa"),
            Color::Rgb(Rgb {
                r: 255,
                g: 0,
                b: 170
            })
        );
    }

    #[test]
    fn named_color_codes_normal_and_bright() {
        // Normal names map to 30–37, bright to 90–97.
        assert_eq!(Style::fg(Color::Named(Named::Black)).prefix(), "\x1b[30m");
        assert_eq!(Style::fg(Color::Named(Named::White)).prefix(), "\x1b[37m");
        assert_eq!(Style::fg(Color::Named(Named::Green)).prefix(), "\x1b[32m");
        assert_eq!(
            Style::fg(Color::Named(Named::BrightGreen)).prefix(),
            "\x1b[92m"
        );
        // Background is foreground + 10, including for the bright range.
        let bg = Style {
            bg: Some(Color::Named(Named::BrightRed)),
            ..Style::default()
        };
        assert_eq!(bg.prefix(), "\x1b[101m");
    }

    #[test]
    fn single_attribute_styles() {
        // Each attribute alone emits exactly its own SGR parameter.
        assert_eq!(
            Style {
                dim: true,
                ..Style::default()
            }
            .prefix(),
            "\x1b[2m"
        );
        assert_eq!(
            Style {
                reverse: true,
                ..Style::default()
            }
            .prefix(),
            "\x1b[7m"
        );
        assert_eq!(
            Style {
                underline: true,
                ..Style::default()
            }
            .prefix(),
            "\x1b[4m"
        );
    }

    #[test]
    fn rgb_serializes_back_to_hex_string() {
        // `Rgb` round-trips through its on-disk `#rrggbb` representation.
        let c = Rgb {
            r: 16,
            g: 32,
            b: 48,
        };
        let repr: RgbRepr = c.into();
        match repr {
            RgbRepr::Hex(s) => assert_eq!(s, "#102030"),
            RgbRepr::Triple { .. } => panic!("Rgb should serialize as hex"),
        }
    }
}
