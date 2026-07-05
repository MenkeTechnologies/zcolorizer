//! Themes: a named mapping from semantic token → [`Style`].
//!
//! Rules decide *which* token a span of text gets (e.g. `date`, `error`, `host`);
//! a theme decides what that token *looks like*. Swapping the active theme
//! recolors everything without touching the rules — this is what "pick the theme
//! from zgui" drives.
//!
//! Tokens are plain strings so the config can invent new ones freely; the
//! well-known names ccze/pygments use are listed in [`tokens`].

use crate::color::{Color, Named, Style};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The canonical semantic token names. Rules tag spans with these; themes style them.
/// Config files may use any string, but these are the ones the builtin rules emit.
pub mod tokens {
    pub const DEFAULT: &str = "default";
    pub const DATE: &str = "date";
    pub const TIME: &str = "time";
    pub const HOST: &str = "host";
    pub const IP: &str = "ip";
    pub const MAC: &str = "mac";
    pub const PID: &str = "pid";
    pub const PROCESS: &str = "process";
    pub const ERROR: &str = "error";
    pub const WARNING: &str = "warning";
    pub const DEBUG: &str = "debug";
    pub const INFO: &str = "info";
    pub const GOOD: &str = "good";
    pub const BAD: &str = "bad";
    pub const SYSTEM: &str = "system";
    pub const KEYWORD: &str = "keyword";
    pub const EMAIL: &str = "email";
    pub const URI: &str = "uri";
    pub const DIR: &str = "dir";
    pub const FILE: &str = "file";
    pub const SIZE: &str = "size";
    pub const VERSION: &str = "version";
    pub const NUMBER: &str = "number";
    pub const ADDRESS: &str = "address";
    pub const PERCENTAGE: &str = "percentage";
    pub const SIGNAL: &str = "signal";
    pub const PROTOCOL: &str = "protocol";
    pub const SERVICE: &str = "service";
    pub const USER: &str = "user";
    pub const HTTP_METHOD: &str = "http_method";
    pub const HTTP_CODE: &str = "http_code";
    pub const STRING: &str = "string";
    pub const BRACKET: &str = "bracket";
    pub const PUNCT: &str = "punct";

    // --- tokens used by the ported ccze format modules ---
    pub const UNKNOWN: &str = "unknown";
    pub const GETTIME: &str = "gettime"; // transfer/elapsed time
    pub const GETSIZE: &str = "getsize"; // transfer size
    pub const FTP_CODE: &str = "ftp_code";
    pub const IDENT: &str = "ident"; // remote user (proxy/http)
    pub const CTYPE: &str = "ctype"; // content type
    pub const SUBJECT: &str = "subject"; // mail subject (procmail)
    pub const FIELD: &str = "field"; // RFC822 header field
    pub const CHAIN: &str = "chain"; // firewall chain (ulogd)
    pub const PKG: &str = "pkg"; // package name (dpkg)
    pub const PKGSTATUS: &str = "pkgstatus"; // package status (dpkg)
    pub const INCOMING: &str = "incoming"; // incoming mail (exim)
    pub const OUTGOING: &str = "outgoing"; // outgoing mail (exim)
    pub const UNIQUE: &str = "unique"; // unique id (exim)
    pub const REPEAT: &str = "repeat"; // "last message repeated N times"
    pub const SWAPNUM: &str = "swapnum"; // squid swap number
                                         // Proxy (squid) action / hierarchy / store-tag tokens
    pub const PROXY_HIT: &str = "proxy_hit";
    pub const PROXY_MISS: &str = "proxy_miss";
    pub const PROXY_DENIED: &str = "proxy_denied";
    pub const PROXY_REFRESH: &str = "proxy_refresh";
    pub const PROXY_SWAPFAIL: &str = "proxy_swapfail";
    pub const PROXY_DIRECT: &str = "proxy_direct";
    pub const PROXY_PARENT: &str = "proxy_parent";
    pub const PROXY_CREATE: &str = "proxy_create";
    pub const PROXY_SWAPIN: &str = "proxy_swapin";
    pub const PROXY_SWAPOUT: &str = "proxy_swapout";
    pub const PROXY_RELEASE: &str = "proxy_release";

    // --- tokens used by the modern format modules ---
    pub const THREAD: &str = "thread"; // thread name/id (app logs)
    pub const LEVEL: &str = "level"; // a generic level word when not error/warn/info/debug
    pub const FACILITY: &str = "facility"; // logger/component/category name
    pub const DURATION: &str = "duration"; // elapsed time (e.g. 12ms, 1.2s)
    pub const LATENCY: &str = "latency"; // request latency
    pub const HASH: &str = "hash"; // hashes, request ids, container ids
    pub const JSON_KEY: &str = "json_key"; // a JSON/structured object key
}

/// A named token→style mapping. `base` is the fallback for any unlisted token
/// (typically the `default` style); `styles` holds the per-token overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Style for the `default` token and any token not present in `styles`.
    #[serde(default)]
    pub base: Style,
    /// Per-token styles, keyed by token name.
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
}

impl Theme {
    /// Resolve the style for a token, falling back to `base` when unmapped.
    pub fn style(&self, token: &str) -> Style {
        self.styles.get(token).copied().unwrap_or(self.base)
    }

    /// Overlay another theme's entries on top of this one (used to merge user
    /// overrides onto a builtin base). `other` wins on conflict.
    pub fn merged_with(mut self, other: &Theme) -> Theme {
        if !other.description.is_empty() {
            self.description = other.description.clone();
        }
        if !other.base.is_plain() {
            self.base = other.base;
        }
        for (k, v) in &other.styles {
            self.styles.insert(k.clone(), *v);
        }
        self
    }
}

/// Build a theme from `(token, style)` pairs plus a name and base style.
fn theme(name: &str, description: &str, base: Style, entries: &[(&str, Style)]) -> Theme {
    Theme {
        name: name.to_string(),
        description: description.to_string(),
        base,
        styles: entries.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    }
}

// ===========================================================================
// The 31 cyberpunk palettes, ported verbatim from iftoprs (itself from
// storageshower). Each is six 256-color indices: (c1..c6) =
// primary, accent (brightest), secondary, mid, dim, darkest.
// ===========================================================================

/// One named palette: a kebab id, a display name, and the six 256-color indices.
#[derive(Clone, Copy)]
struct Palette {
    name: &'static str,
    display: &'static str,
    c: [u8; 6],
}

/// All 31 palettes, in iftoprs declaration order (NeonSprawl is the default).
const PALETTES: &[Palette] = &[
    Palette {
        name: "neon-sprawl",
        display: "Neon Sprawl",
        c: [27, 48, 135, 141, 63, 99],
    },
    Palette {
        name: "acid-rain",
        display: "Acid Rain",
        c: [28, 46, 34, 40, 22, 35],
    },
    Palette {
        name: "ice-breaker",
        display: "Ice Breaker",
        c: [19, 39, 25, 33, 21, 32],
    },
    Palette {
        name: "synth-wave",
        display: "Synth Wave",
        c: [91, 177, 128, 134, 93, 97],
    },
    Palette {
        name: "rust-belt",
        display: "Rust Belt",
        c: [172, 214, 178, 220, 166, 130],
    },
    Palette {
        name: "ghost-wire",
        display: "Ghost Wire",
        c: [37, 50, 44, 87, 30, 23],
    },
    Palette {
        name: "red-sector",
        display: "Red Sector",
        c: [160, 203, 196, 210, 124, 88],
    },
    Palette {
        name: "sakura-den",
        display: "Sakura Den",
        c: [175, 218, 182, 225, 169, 132],
    },
    Palette {
        name: "data-stream",
        display: "Data Stream",
        c: [22, 46, 28, 119, 34, 22],
    },
    Palette {
        name: "solar-flare",
        display: "Solar Flare",
        c: [202, 220, 196, 213, 160, 125],
    },
    Palette {
        name: "neon-noir",
        display: "Neon Noir",
        c: [201, 231, 93, 219, 57, 53],
    },
    Palette {
        name: "chrome-heart",
        display: "Chrome Heart",
        c: [250, 255, 246, 253, 243, 239],
    },
    Palette {
        name: "blade-runner",
        display: "Blade Runner",
        c: [208, 37, 166, 73, 130, 23],
    },
    Palette {
        name: "void-walker",
        display: "Void Walker",
        c: [55, 99, 54, 141, 92, 17],
    },
    Palette {
        name: "toxic-waste",
        display: "Toxic Waste",
        c: [118, 190, 154, 226, 82, 58],
    },
    Palette {
        name: "cyber-frost",
        display: "Cyber Frost",
        c: [159, 195, 153, 189, 111, 67],
    },
    Palette {
        name: "plasma-core",
        display: "Plasma Core",
        c: [199, 213, 163, 207, 126, 89],
    },
    Palette {
        name: "steel-nerve",
        display: "Steel Nerve",
        c: [68, 110, 60, 146, 24, 236],
    },
    Palette {
        name: "dark-signal",
        display: "Dark Signal",
        c: [30, 43, 23, 79, 29, 16],
    },
    Palette {
        name: "glitch-pop",
        display: "Glitch Pop",
        c: [201, 51, 226, 47, 196, 21],
    },
    Palette {
        name: "holo-shift",
        display: "Holo Shift",
        c: [123, 219, 159, 183, 87, 133],
    },
    Palette {
        name: "night-city",
        display: "Night City",
        c: [214, 227, 209, 223, 172, 94],
    },
    Palette {
        name: "deep-net",
        display: "Deep Net",
        c: [19, 33, 17, 75, 26, 16],
    },
    Palette {
        name: "laser-grid",
        display: "Laser Grid",
        c: [46, 201, 51, 226, 196, 21],
    },
    Palette {
        name: "quantum-flux",
        display: "Quantum Flux",
        c: [135, 75, 171, 111, 98, 61],
    },
    Palette {
        name: "bio-hazard",
        display: "Bio Hazard",
        c: [148, 184, 106, 192, 64, 22],
    },
    Palette {
        name: "darkwave",
        display: "Darkwave",
        c: [53, 140, 89, 176, 127, 52],
    },
    Palette {
        name: "overlock",
        display: "Overlock",
        c: [196, 208, 160, 214, 124, 52],
    },
    Palette {
        name: "megacorp",
        display: "Megacorp",
        c: [252, 39, 245, 81, 242, 236],
    },
    Palette {
        name: "zaibatsu",
        display: "Zaibatsu",
        c: [167, 216, 131, 224, 95, 52],
    },
    Palette {
        name: "iftopcolor",
        display: "iftopcolor",
        c: [21, 46, 28, 48, 33, 19],
    },
];

/// Shift an indexed 256-color one step lighter (ported from iftoprs) — used to
/// derive an extra distinguishable shade so tokens sharing a palette slot still
/// read apart.
fn shift_lighter(c: u8) -> u8 {
    if c >= 232 {
        c.saturating_add(2)
    } else if c >= 16 {
        let idx = c - 16;
        let b = idx % 6;
        let g = (idx / 6) % 6;
        let r = idx / 36;
        16 + (r + 1).min(5) * 36 + (g + 1).min(5) * 6 + (b + 1).min(5)
    } else {
        c.saturating_add(8).min(15)
    }
}

/// Build a full token→style [`Theme`] from a six-color palette by assigning each
/// semantic token to a palette role (with bold/underline/italic to multiply the
/// six colors into enough distinct looks for ~60 tokens). Severity tokens lean on
/// the brightest accent so errors stay loud whatever the palette.
fn theme_from_palette(p: &Palette) -> Theme {
    use tokens::*;
    let [c1, c2, c3, c4, c5, _c6] = p.c;
    let pl = shift_lighter(c3); // a derived shade for process vs. host

    let i = |n: u8| Style::fg(Color::idx(n));
    let bi = |n: u8| Style::bold_fg(Color::idx(n));
    let ui = |n: u8| Style {
        underline: true,
        ..Style::fg(Color::idx(n))
    };
    let it = |n: u8| Style {
        italic: true,
        ..Style::fg(Color::idx(n))
    };

    let entries: Vec<(&str, Style)> = vec![
        (DATE, bi(c2)),
        (TIME, i(c2)),
        (HOST, bi(c3)),
        (IP, i(c3)),
        (MAC, i(c5)),
        (PID, bi(c4)),
        (PROCESS, bi(pl)),
        (ERROR, bi(c2)),
        (WARNING, bi(c4)),
        (DEBUG, i(c5)),
        // INFO leans on the same primary hue but bold, so it reads apart from the
        // base text (which is also c1) instead of vanishing into it.
        (INFO, bi(c1)),
        (GOOD, bi(c3)),
        (BAD, bi(c2)),
        (SYSTEM, i(c4)),
        (KEYWORD, bi(c2)),
        (EMAIL, i(c3)),
        (URI, ui(c3)),
        (DIR, i(c4)),
        (FILE, i(c1)),
        (SIZE, i(c3)),
        (VERSION, i(c4)),
        (NUMBER, i(c4)),
        (ADDRESS, i(c5)),
        (PERCENTAGE, bi(c3)),
        (SIGNAL, bi(c2)),
        (PROTOCOL, i(c3)),
        (SERVICE, i(c3)),
        (USER, bi(c4)),
        (HTTP_METHOD, bi(c3)),
        (HTTP_CODE, bi(c4)),
        (STRING, i(c3)),
        (BRACKET, i(c5)),
        (PUNCT, i(c5)),
        (UNKNOWN, i(c5)),
        (GETTIME, i(c4)),
        (GETSIZE, i(c3)),
        (FTP_CODE, bi(c4)),
        (IDENT, i(c4)),
        (CTYPE, i(c3)),
        (SUBJECT, bi(c2)),
        (FIELD, i(c5)),
        (CHAIN, bi(c4)),
        (PKG, bi(c3)),
        (PKGSTATUS, bi(c4)),
        (INCOMING, bi(c3)),
        (OUTGOING, bi(c2)),
        (UNIQUE, i(c5)),
        (REPEAT, it(c5)),
        (SWAPNUM, i(c4)),
        (PROXY_HIT, bi(c3)),
        (PROXY_MISS, bi(c4)),
        (PROXY_DENIED, bi(c2)),
        (PROXY_REFRESH, i(c1)),
        (PROXY_SWAPFAIL, bi(c2)),
        (PROXY_DIRECT, i(c3)),
        (PROXY_PARENT, i(c4)),
        (PROXY_CREATE, i(c3)),
        (PROXY_SWAPIN, i(c1)),
        (PROXY_SWAPOUT, i(c4)),
        (PROXY_RELEASE, i(c2)),
        // modern format tokens
        (THREAD, i(c5)),
        (LEVEL, bi(c4)),
        (FACILITY, i(pl)),
        (DURATION, i(c4)),
        (LATENCY, i(c4)),
        (HASH, i(c5)),
        (JSON_KEY, i(c3)),
    ];

    Theme {
        name: p.name.to_string(),
        description: format!("Cyberpunk palette — {}", p.display),
        base: i(c1),
        styles: entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    }
}

/// The flagship cyberpunk theme (the iftoprs default palette, **Neon Sprawl**).
pub fn cyberpunk() -> Theme {
    theme_from_palette(&PALETTES[0])
}

/// A faithful approximation of classic ccze's 16-color scheme, for nostalgia/parity.
pub fn ccze_classic() -> Theme {
    use tokens::*;
    let n = |c: Named| Style::fg(Color::Named(c));
    let b = |c: Named| Style::bold_fg(Color::Named(c));
    theme(
        "ccze-classic",
        "The original ccze 16-color palette",
        n(Named::Cyan),
        &[
            (DATE, b(Named::Cyan)),
            (TIME, b(Named::Cyan)),
            (HOST, b(Named::Blue)),
            (IP, b(Named::Blue)),
            (MAC, b(Named::Cyan)),
            (PID, b(Named::Yellow)),
            (PROCESS, b(Named::Green)),
            (ERROR, b(Named::Red)),
            (WARNING, b(Named::Yellow)),
            (DEBUG, n(Named::Blue)),
            (INFO, n(Named::Cyan)),
            (GOOD, b(Named::Green)),
            (BAD, b(Named::Red)),
            (SYSTEM, b(Named::Yellow)),
            (KEYWORD, b(Named::Magenta)),
            (EMAIL, b(Named::Green)),
            (URI, b(Named::Green)),
            (DIR, b(Named::Cyan)),
            (FILE, n(Named::White)),
            (SIZE, b(Named::White)),
            (VERSION, b(Named::White)),
            (NUMBER, b(Named::White)),
            (ADDRESS, b(Named::Cyan)),
            (PERCENTAGE, b(Named::Green)),
            (SIGNAL, b(Named::Red)),
            (PROTOCOL, b(Named::Magenta)),
            (SERVICE, b(Named::Magenta)),
            (USER, b(Named::Yellow)),
            (HTTP_METHOD, b(Named::Green)),
            (HTTP_CODE, b(Named::Yellow)),
            (NUMBER, b(Named::White)),
        ],
    )
}

/// All themes that ship in the binary: the 31 ported palettes (Neon Sprawl first,
/// the default), plus `ccze-classic` for 16-color parity.
pub fn builtins() -> Vec<Theme> {
    PALETTES
        .iter()
        .map(theme_from_palette)
        .chain(std::iter::once(ccze_classic()))
        .collect()
}

/// The default theme name when nothing else is specified.
pub const DEFAULT_THEME: &str = "neon-sprawl";

/// Normalize a user-supplied theme name to a builtin id: case-insensitive, and
/// accepting display names ("Neon Sprawl") and the `cyberpunk` alias.
fn normalize(name: &str) -> String {
    let n = name.trim().to_ascii_lowercase();
    if n == "cyberpunk" || n == "default" {
        return DEFAULT_THEME.to_string();
    }
    // collapse spaces/underscores to hyphens so "Neon Sprawl" / "neon_sprawl" match.
    n.chars()
        .map(|c| if c == ' ' || c == '_' { '-' } else { c })
        .collect()
}

/// Look up a builtin theme by name (case-insensitive; accepts display names and
/// the `cyberpunk` alias for the default Neon Sprawl palette).
pub fn builtin(name: &str) -> Option<Theme> {
    let want = normalize(name);
    builtins().into_iter().find(|t| t.name == want)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neon_sprawl_is_default_first() {
        assert_eq!(builtins()[0].name, DEFAULT_THEME);
    }

    #[test]
    fn all_31_palettes_present() {
        assert_eq!(PALETTES.len(), 31);
        // builtins = 31 palettes + ccze-classic
        assert_eq!(builtins().len(), 32);
    }

    #[test]
    fn cyberpunk_alias_resolves_to_neon_sprawl() {
        assert_eq!(builtin("cyberpunk").unwrap().name, "neon-sprawl");
        assert_eq!(builtin("Neon Sprawl").unwrap().name, "neon-sprawl");
    }

    #[test]
    fn every_palette_builds_and_styles_error_bold() {
        for p in PALETTES {
            let t = theme_from_palette(p);
            assert!(t.style(tokens::ERROR).bold, "{} error not bold", p.name);
            assert!(t.style(tokens::DEFAULT).fg.is_some());
        }
    }

    #[test]
    fn unmapped_token_uses_base() {
        let t = cyberpunk();
        assert_eq!(t.style("nonexistent-token"), t.base);
    }

    #[test]
    fn ccze_classic_is_a_builtin() {
        assert!(builtin("ccze-classic").is_some());
        assert_eq!(ccze_classic().name, "ccze-classic");
    }

    #[test]
    fn builtin_unknown_returns_none() {
        assert!(builtin("no-such-theme").is_none());
    }

    #[test]
    fn normalize_accepts_spaces_underscores_and_default_alias() {
        // Spaces and underscores both collapse to hyphens; `default`/`cyberpunk` alias.
        assert_eq!(builtin("neon_sprawl").unwrap().name, "neon-sprawl");
        assert_eq!(builtin("  NEON SPRAWL  ").unwrap().name, "neon-sprawl");
        assert_eq!(builtin("default").unwrap().name, DEFAULT_THEME);
        assert_eq!(builtin("Acid Rain").unwrap().name, "acid-rain");
    }

    #[test]
    fn merged_with_overlays_styles_and_keeps_unset_base() {
        use crate::color::{Color, Named};
        let base = theme(
            "base",
            "base desc",
            Style::fg(Color::Named(Named::Cyan)),
            &[
                (tokens::ERROR, Style::fg(Color::Named(Named::Red))),
                (tokens::INFO, Style::fg(Color::Named(Named::Blue))),
            ],
        );
        // `other` has a plain base (so base is preserved), no description (kept),
        // and overrides only ERROR while adding GOOD.
        let other = theme(
            "base",
            "",
            Style::plain(),
            &[
                (tokens::ERROR, Style::bold_fg(Color::Named(Named::Magenta))),
                (tokens::GOOD, Style::fg(Color::Named(Named::Green))),
            ],
        );
        let merged = base.merged_with(&other);
        // base preserved (other's base was plain), description preserved (other empty).
        assert_eq!(merged.base, Style::fg(Color::Named(Named::Cyan)));
        assert_eq!(merged.description, "base desc");
        // ERROR overridden, INFO untouched, GOOD added.
        assert_eq!(
            merged.style(tokens::ERROR),
            Style::bold_fg(Color::Named(Named::Magenta))
        );
        assert_eq!(
            merged.style(tokens::INFO),
            Style::fg(Color::Named(Named::Blue))
        );
        assert_eq!(
            merged.style(tokens::GOOD),
            Style::fg(Color::Named(Named::Green))
        );
    }

    #[test]
    fn merged_with_nonplain_base_and_description_win() {
        use crate::color::{Color, Named};
        let base = theme("b", "old", Style::fg(Color::Named(Named::Cyan)), &[]);
        let other = theme("b", "new", Style::fg(Color::Named(Named::Yellow)), &[]);
        let merged = base.merged_with(&other);
        assert_eq!(merged.base, Style::fg(Color::Named(Named::Yellow)));
        assert_eq!(merged.description, "new");
    }

    #[test]
    fn process_and_host_differ_via_lightened_shade() {
        // PROCESS is a lightened shade of the host slot so the two read apart.
        let t = cyberpunk();
        assert_ne!(t.style(tokens::PROCESS), t.style(tokens::HOST));
    }

    #[test]
    fn builtin_theme_names_are_unique() {
        let names: Vec<String> = builtins().into_iter().map(|t| t.name).collect();
        let mut dedup = names.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(
            dedup.len(),
            names.len(),
            "builtin theme names must be unique"
        );
    }

    #[test]
    fn shift_lighter_clamps_at_palette_edges() {
        // Grayscale ramp saturates near the top instead of overflowing.
        assert_eq!(shift_lighter(255), 255);
        // A low 16-color index steps up but stays in range.
        assert!(shift_lighter(1) <= 15);
        // A mid-cube color shifts to a different (lighter) cube index.
        assert_ne!(shift_lighter(16), 16);
    }

    #[test]
    fn every_builtin_name_resolves_back() {
        // Round-trip: each builtin's own name normalizes to itself.
        for t in builtins() {
            assert_eq!(
                builtin(&t.name).map(|b| b.name).as_deref(),
                Some(t.name.as_str())
            );
        }
    }
}
