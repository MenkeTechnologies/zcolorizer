//! Configuration: the on-disk TOML that makes rules and themes fully customizable.
//!
//! A config can:
//!   * pick the active theme by name (`theme = "cyberpunk"`),
//!   * define or override themes (`[[themes]]`),
//!   * replace or extend the rule list (`[[rules]]`),
//!   * tweak behavior (`rules_mode`, etc.).
//!
//! Anything omitted falls back to the builtins, so a one-line config that only
//! sets `theme = "..."` is valid. The default search path is
//! `$XDG_CONFIG_HOME/zcolorizer/config.toml` (a.k.a. `~/.config/...`).

use crate::error::{Error, Result};
use crate::rules::RuleDef;
use crate::theme::Theme;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How the config's `[[rules]]` combine with the builtin generic ruleset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RulesMode {
    /// Config rules first, then builtins (config rules win on overlap). Default —
    /// a user adding a specific rule expects it to beat the generic builtins.
    #[default]
    Prepend,
    /// Builtin rules first, then the config's rules (builtins win on overlap).
    Extend,
    /// Only the config's rules; builtins are dropped entirely.
    Replace,
}

/// The parsed config document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    /// Name of the active theme. Defaults to `cyberpunk`.
    pub theme: Option<String>,
    /// How `rules` combine with the builtins.
    pub rules_mode: RulesMode,
    /// User-defined / override themes. Merged onto same-named builtins.
    pub themes: Vec<Theme>,
    /// User rules (combined with builtins per `rules_mode`).
    pub rules: Vec<RuleDef>,
    /// Format modules to enable (ccze plugin ports: `httpd`, `squid`, …, or `all`).
    /// Their rules sit between user rules and the generic builtins.
    pub modules: Vec<String>,
}

impl Config {
    /// The default per-user config path, if a config dir can be determined.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("zcolorizer").join("config.toml"))
    }

    /// Load and parse a config from `path`.
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Config::parse(&text, path)
    }

    /// Parse config text (the `path` is only used for error messages).
    pub fn parse(text: &str, path: &Path) -> Result<Config> {
        toml::from_str(text).map_err(|source| Error::ConfigParse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Load the default-path config if it exists; otherwise an empty config.
    pub fn load_default() -> Result<Config> {
        match Config::default_path() {
            Some(p) if p.exists() => Config::load(&p),
            _ => Ok(Config::default()),
        }
    }

    /// Resolve the effective theme: a builtin (possibly overridden by a same-named
    /// `[[themes]]` entry) or a wholly config-defined theme. `name` overrides
    /// `self.theme`; both default to `cyberpunk`.
    pub fn resolve_theme(&self, name: Option<&str>) -> Result<Theme> {
        let want = name
            .or(self.theme.as_deref())
            .unwrap_or(crate::theme::DEFAULT_THEME);

        let user = self
            .themes
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(want));
        match crate::theme::builtin(want) {
            Some(base) => Ok(match user {
                Some(u) => base.merged_with(u),
                None => base,
            }),
            None => user
                .cloned()
                .ok_or_else(|| Error::UnknownTheme(want.to_string())),
        }
    }

    /// The effective ordered rule-definition list. Composition: the enabled format
    /// modules' rules always sit between the user rules and the generic builtins
    /// (so a format module beats the generic word-colorizer but a user rule still
    /// wins). `rules_mode` controls where the user rules go relative to generics.
    /// Unknown module names are ignored here — validate with [`Config::unknown_modules`].
    pub fn resolve_rule_defs(&self) -> Vec<RuleDef> {
        let generic = crate::rules::builtin_generic();
        let module_rules = crate::modules::resolve(&self.modules).unwrap_or_else(|_| {
            // Drop only the unknown names; keep the valid ones.
            let known: Vec<String> = self
                .modules
                .iter()
                .filter(|n| n.eq_ignore_ascii_case("all") || crate::modules::get(n).is_some())
                .cloned()
                .collect();
            crate::modules::resolve(&known).unwrap_or_default()
        });

        match self.rules_mode {
            RulesMode::Replace => {
                let mut v = self.rules.clone();
                v.extend(module_rules);
                v
            }
            RulesMode::Extend => {
                let mut v = module_rules;
                v.extend(generic);
                v.extend(self.rules.iter().cloned());
                v
            }
            RulesMode::Prepend => {
                let mut v = self.rules.clone();
                v.extend(module_rules);
                v.extend(generic);
                v
            }
        }
    }

    /// Any requested module names that aren't known (for CLI validation).
    pub fn unknown_modules(&self) -> Vec<String> {
        match crate::modules::resolve(&self.modules) {
            Ok(_) => Vec::new(),
            Err(unknown) => unknown,
        }
    }

    /// Every theme name available to this config (builtins + user-defined), deduped,
    /// with builtins first. This is what a theme picker enumerates.
    pub fn available_theme_names(&self) -> Vec<String> {
        let mut names: Vec<String> = crate::theme::builtins()
            .into_iter()
            .map(|t| t.name)
            .collect();
        for t in &self.themes {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&t.name)) {
                names.push(t.name.clone());
            }
        }
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_resolves_default_theme() {
        let c = Config::default();
        let t = c.resolve_theme(None).unwrap();
        assert_eq!(t.name, crate::theme::DEFAULT_THEME);
    }

    #[test]
    fn cyberpunk_alias_still_works() {
        let c = Config::default();
        assert_eq!(
            c.resolve_theme(Some("cyberpunk")).unwrap().name,
            "neon-sprawl"
        );
    }

    #[test]
    fn module_rules_sit_before_generic() {
        let mut c = Config::default();
        c.modules.push("syslog".into());
        let defs = c.resolve_rule_defs();
        let syslog_idx = defs.iter().position(|d| d.name == "syslog-line").unwrap();
        let number_idx = defs.iter().position(|d| d.name == "number").unwrap();
        assert!(
            syslog_idx < number_idx,
            "module rule must precede generic number rule"
        );
    }

    #[test]
    fn name_override_beats_config_theme() {
        let c = Config {
            theme: Some("cyberpunk".into()),
            ..Config::default()
        };
        let t = c.resolve_theme(Some("ccze-classic")).unwrap();
        assert_eq!(t.name, "ccze-classic");
    }

    #[test]
    fn extend_mode_appends_user_rules() {
        let mut c = Config {
            rules_mode: RulesMode::Extend,
            ..Config::default()
        };
        c.rules.push(RuleDef::with_token("x", "FOO", "error"));
        let defs = c.resolve_rule_defs();
        assert!(defs.len() > 1);
        assert_eq!(defs.last().unwrap().name, "x");
    }

    #[test]
    fn prepend_is_default_and_user_rules_lead() {
        let mut c = Config::default();
        c.rules.push(RuleDef::with_token("x", "FOO", "error"));
        let defs = c.resolve_rule_defs();
        assert_eq!(c.rules_mode, RulesMode::Prepend);
        assert_eq!(defs.first().unwrap().name, "x");
    }

    #[test]
    fn replace_mode_drops_builtins() {
        let mut c = Config {
            rules_mode: RulesMode::Replace,
            ..Config::default()
        };
        c.rules.push(RuleDef::with_token("x", "FOO", "error"));
        let defs = c.resolve_rule_defs();
        assert_eq!(defs.len(), 1);
    }

    #[test]
    fn parse_minimal_toml() {
        let c = Config::parse("theme = \"ccze-classic\"\n", Path::new("test")).unwrap();
        assert_eq!(c.theme.as_deref(), Some("ccze-classic"));
    }

    #[test]
    fn unknown_modules_reported_and_known_ones_kept() {
        let c = Config {
            modules: vec!["syslog".into(), "bogus".into(), "httpd".into()],
            ..Config::default()
        };
        // Only the bogus name is flagged.
        assert_eq!(c.unknown_modules(), vec!["bogus".to_string()]);
        // resolve_rule_defs drops the unknown but still includes the valid modules.
        let defs = c.resolve_rule_defs();
        assert!(defs.iter().any(|d| d.name == "syslog-line"), "syslog kept");
        assert!(defs.iter().any(|d| d.name == "httpd-access"), "httpd kept");
    }

    #[test]
    fn no_modules_means_none_unknown() {
        assert!(Config::default().unknown_modules().is_empty());
    }

    #[test]
    fn available_theme_names_lists_builtins_first_and_dedups() {
        let mut c = Config::default();
        let builtin_count = c.available_theme_names().len();
        assert_eq!(builtin_count, 32, "31 palettes + ccze-classic");
        assert_eq!(
            c.available_theme_names().first().unwrap(),
            crate::theme::DEFAULT_THEME
        );

        // A user theme with a fresh name is appended; one that shadows a builtin
        // (case-insensitively) does not grow the list.
        c.themes.push(Theme {
            name: "My Custom".into(),
            description: String::new(),
            base: Default::default(),
            styles: Default::default(),
        });
        c.themes.push(Theme {
            name: "NEON-SPRAWL".into(),
            description: String::new(),
            base: Default::default(),
            styles: Default::default(),
        });
        let names = c.available_theme_names();
        assert_eq!(
            names.len(),
            builtin_count + 1,
            "only the fresh name was added"
        );
        assert!(names.iter().any(|n| n == "My Custom"));
    }

    #[test]
    fn rules_mode_defaults_to_prepend() {
        assert_eq!(RulesMode::default(), RulesMode::Prepend);
        assert_eq!(Config::default().rules_mode, RulesMode::Prepend);
    }

    #[test]
    fn unknown_theme_name_errors() {
        let c = Config::default();
        let err = c.resolve_theme(Some("does-not-exist")).unwrap_err();
        assert!(matches!(err, Error::UnknownTheme(name) if name == "does-not-exist"));
    }

    #[test]
    fn parse_rejects_malformed_toml() {
        let err = Config::parse("theme = = nope", Path::new("bad.toml")).unwrap_err();
        assert!(matches!(err, Error::ConfigParse { .. }));
    }

    #[test]
    fn color_forms_deserialize_from_toml() {
        use crate::color::{Color, Named, Rgb};
        // Exercises every on-disk Color form: named string, indexed table,
        // hex string, and rgb-triple table.
        let toml = r##"
theme = "custom-x"
[[themes]]
name = "custom-x"
base = { fg = "red" }
[themes.styles.a]
fg = { index = 213 }
[themes.styles.b]
fg = "#ff00aa"
[themes.styles.c]
fg = { rgb = [1, 2, 3] }
"##;
        let c = Config::parse(toml, Path::new("t")).unwrap();
        let t = c.resolve_theme(None).unwrap();
        assert_eq!(t.base.fg, Some(Color::Named(Named::Red)));
        assert_eq!(t.style("a").fg, Some(Color::idx(213)));
        assert_eq!(t.style("b").fg, Some(Color::hex("#ff00aa")));
        assert_eq!(t.style("c").fg, Some(Color::Rgb(Rgb { r: 1, g: 2, b: 3 })));
    }

    #[test]
    fn extend_mode_orders_modules_then_generic_then_user() {
        let mut c = Config {
            rules_mode: RulesMode::Extend,
            ..Config::default()
        };
        c.modules.push("syslog".into());
        c.rules.push(RuleDef::with_token("user-x", "FOO", "error"));
        let defs = c.resolve_rule_defs();
        let syslog_i = defs.iter().position(|d| d.name == "syslog-line").unwrap();
        let number_i = defs.iter().position(|d| d.name == "number").unwrap();
        let user_i = defs.iter().position(|d| d.name == "user-x").unwrap();
        assert!(syslog_i < number_i, "module rules precede generic");
        assert!(number_i < user_i, "user rules come last in extend mode");
    }

    #[test]
    fn load_missing_file_is_io_error() {
        let err = Config::load(Path::new("/no/such/dir/zcolorizer-cfg.toml")).unwrap_err();
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn load_reads_and_parses_a_file() {
        let path =
            std::env::temp_dir().join(format!("zcolorizer-load-{}.toml", std::process::id()));
        std::fs::write(&path, "theme = \"ccze-classic\"\n").unwrap();
        let loaded = Config::load(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(loaded.unwrap().theme.as_deref(), Some("ccze-classic"));
    }

    #[test]
    fn user_theme_overrides_builtin_token() {
        let toml = r##"
theme = "cyberpunk"
[[themes]]
name = "cyberpunk"
[themes.styles.error]
fg = "#123456"
"##;
        let c = Config::parse(toml, Path::new("test")).unwrap();
        let t = c.resolve_theme(None).unwrap();
        assert_eq!(
            t.style("error").fg,
            Some(crate::color::Color::hex("#123456"))
        );
        // a non-overridden token still has its builtin value
        assert!(t.style("good").fg.is_some());
    }
}
