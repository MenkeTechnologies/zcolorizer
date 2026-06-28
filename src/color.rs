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
            RgbRepr::Triple { rgb } => Rgb { r: rgb[0], g: rgb[1], b: rgb[2] },
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
        Style { fg: Some(color), ..Style::default() }
    }

    pub fn bold_fg(color: Color) -> Style {
        Style { fg: Some(color), bold: true, ..Style::default() }
    }

    /// True when this style would emit no escape at all.
    pub fn is_plain(&self) -> bool {
        *self == Style::default()
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
        assert_eq!(Rgb::from_hex("#ff00aa"), Some(Rgb { r: 255, g: 0, b: 170 }));
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
        let s = Style { bg: Some(Color::Named(Named::Blue)), ..Style::default() };
        assert_eq!(s.prefix(), "\x1b[44m");
    }
}
