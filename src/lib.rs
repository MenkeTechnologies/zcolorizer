//! # zcolorizer
//!
//! A real-time log colorizer — a Rust port of [ccze] and the pygments
//! regex→token idea — with **fully customizable rules** and **swappable themes**.
//!
//! The pipeline is three decoupled pieces:
//!
//! 1. [`rules`] — regexes that tag spans of a line with a semantic *token*
//!    (`date`, `error`, `host`, …). Named capture groups make one regex paint
//!    several fields. Rules live in the config, so users own them entirely.
//! 2. [`theme`] — a named map from token → [`color::Style`]. The active theme
//!    decides what each token looks like; swapping it recolors everything
//!    (the "pick the theme from zgui" flow). The flagship is [`theme::cyberpunk`].
//! 3. [`engine::Colorizer`] — runs the compiled rules over each line and paints
//!    the claimed spans with the theme.
//!
//! [`config::Config`] ties them together from a TOML file.
//!
//! ## Quick start
//!
//! ```
//! use zcolorizer::Colorizer;
//! let cz = Colorizer::from_config(&zcolorizer::config::Config::default(), None).unwrap();
//! let painted = cz.colorize_line("Jun 27 14:03:11 host sshd[1234]: ERROR bad login");
//! assert!(painted.contains("\x1b["));
//! ```
//!
//! [ccze]: https://github.com/cornet/ccze

pub mod color;
pub mod config;
pub mod engine;
pub mod error;
pub mod modules;
pub mod modules_modern;
pub mod novelty;
pub mod rules;
pub mod theme;

pub use color::{Color, Style};
pub use config::Config;
pub use engine::Colorizer;
pub use error::{Error, Result};
pub use novelty::NoveltyModel;
pub use theme::Theme;

impl Colorizer {
    /// Build a ready-to-run colorizer from a [`Config`], optionally overriding the
    /// theme by name. Compiles the effective ruleset and resolves the theme.
    pub fn from_config(config: &Config, theme_name: Option<&str>) -> Result<Colorizer> {
        let defs = config.resolve_rule_defs();
        let rules = rules::compile_all(&defs)?;
        let theme = config.resolve_theme(theme_name)?;
        Ok(Colorizer::new(rules, theme))
    }

    /// Convenience: the default builtin colorizer (generic rules + cyberpunk theme).
    pub fn default_cyberpunk() -> Colorizer {
        Colorizer::from_config(&Config::default(), None)
            .expect("builtin rules and theme are always valid")
    }
}
