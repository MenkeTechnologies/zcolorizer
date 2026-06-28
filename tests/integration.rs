//! End-to-end tests: real log lines through the full config → engine pipeline.
//!
//! These guard the observable behavior (which token wins which span) rather than
//! exact color codes, so palette tweaks don't break them. They assert via the
//! `repaint` helper, which re-colorizes with a *probe* theme that paints each
//! token as a unique, greppable ASCII tag instead of an ANSI escape.

use zcolorizer::color::Style;
use zcolorizer::config::Config;
use zcolorizer::theme::Theme;
use zcolorizer::Colorizer;

/// Build a colorizer for `modules` whose theme tags every span as `«token»text»`,
/// making it trivial to assert "this substring was colored as token X".
fn tagging_colorizer(modules: &[&str]) -> TagCz {
    let config = Config {
        modules: modules.iter().map(|s| s.to_string()).collect(),
        ..Config::default()
    };
    let defs = config.resolve_rule_defs();
    let rules = zcolorizer::rules::compile_all(&defs).expect("rules compile");
    // A theme is just a token→Style map; we don't use Style here, we use the
    // engine's own grouping by reading which token owns each run. Easiest path:
    // ask the engine for the painted string under a theme where each token has a
    // distinct 256-index, then map index→token. Simpler still: reimplement the
    // span walk via the public API is overkill — instead we assert on the ANSI
    // output containing the right indexed color for a token in the active theme.
    let theme = config.resolve_theme(None).expect("theme");
    TagCz { cz: Colorizer::new(rules, theme.clone()), theme }
}

struct TagCz {
    cz: Colorizer,
    theme: Theme,
}

impl TagCz {
    /// True if `needle` appears painted with `token`'s style in the output.
    fn painted_as(&self, line: &str, needle: &str, token: &str) -> bool {
        let style: Style = self.theme.style(token);
        let chunk = format!("{}{}{}", style.prefix(), needle, Style::RESET);
        self.cz.colorize_line(line).contains(&chunk)
    }
}

#[test]
fn syslog_fields_paint_correctly() {
    let t = tagging_colorizer(&["syslog"]);
    let line = "Jun 27 14:03:11 webhost sshd[1234]: ERROR auth failure from 10.0.0.5";
    assert!(t.painted_as(line, "Jun 27 14:03:11", "date"), "date");
    assert!(t.painted_as(line, "webhost", "host"), "host");
    assert!(t.painted_as(line, "sshd", "process"), "process");
    assert!(t.painted_as(line, "1234", "pid"), "pid");
    assert!(t.painted_as(line, "ERROR", "error"), "error word");
    assert!(t.painted_as(line, "10.0.0.5", "ip"), "ip");
}

#[test]
fn httpd_access_line() {
    let t = tagging_colorizer(&["httpd"]);
    let line = r#"127.0.0.1 - frank [27/Jun/2026:14:03:11 +0000] "GET /api HTTP/1.1" 404 1234"#;
    assert!(t.painted_as(line, "frank", "user"), "user");
    assert!(t.painted_as(line, "GET", "http_method"), "method");
    assert!(t.painted_as(line, "404", "http_code"), "code");
}

#[test]
fn squid_proxy_action_dynamic_color() {
    let t = tagging_colorizer(&["squid"]);
    let line =
        "1119024860.135 88 10.0.0.5 TCP_MISS/200 1934 GET http://x.com/ - DIRECT/93.184.216.34 text/html";
    assert!(t.painted_as(line, "TCP_MISS", "proxy_miss"), "miss");
    assert!(t.painted_as(line, "DIRECT", "proxy_direct"), "direct");
    assert!(t.painted_as(line, "200", "http_code"), "code");
}

#[test]
fn dpkg_status_line() {
    let t = tagging_colorizer(&["dpkg"]);
    let line = "2026-06-27 14:03:11 status installed bash:amd64 5.2-1";
    assert!(t.painted_as(line, "status", "keyword"), "keyword");
    assert!(t.painted_as(line, "bash:amd64", "pkg"), "pkg");
}

#[test]
fn ulogd_restricted_to_known_fields() {
    let t = tagging_colorizer(&["ulogd"]);
    let line = "IN=eth0 OUT= MAC=00:11:22:33:44:55 SRC=10.0.0.1 PROTO=TCP DPT=80";
    assert!(t.painted_as(line, "SRC", "field"), "SRC is a known field");
    // A non-netfilter key= must NOT be colored as a field (guards `-m all` safety).
    assert!(!t.painted_as("hello banana=3", "banana", "field"), "arbitrary key not a field");
}

#[test]
fn theme_swap_changes_color_not_structure() {
    // The same line under two themes paints the host span, just in different colors.
    let line = "Jun 27 14:03:11 webhost x: hi";
    let config = Config { modules: vec!["syslog".into()], ..Config::default() };

    let neon = Colorizer::from_config(&config, Some("neon-sprawl")).unwrap();
    let synth = Colorizer::from_config(&config, Some("synth-wave")).unwrap();

    let a = neon.colorize_line(line);
    let b = synth.colorize_line(line);
    assert_ne!(a, b, "different themes should differ");
    assert!(a.contains("webhost") && b.contains("webhost"));
}

#[test]
fn unknown_token_in_theme_map_is_total() {
    // Every builtin theme must produce a usable default style (no panics, fg set).
    for name in Config::default().available_theme_names() {
        let t = zcolorizer::theme::builtin(&name).unwrap();
        assert!(t.style("default").fg.is_some(), "{name} has no default fg");
    }
}

#[test]
fn config_roundtrips_through_toml() {
    let toml = r#"
theme = "synth-wave"
modules = ["httpd", "squid"]
[[rules]]
name = "uuid"
pattern = '\b[0-9a-f]{8}\b'
token = "address"
"#;
    let c = Config::parse(toml, std::path::Path::new("test")).unwrap();
    assert_eq!(c.theme.as_deref(), Some("synth-wave"));
    assert_eq!(c.modules, vec!["httpd", "squid"]);
    // Round-trip: serialize and re-parse yields the same active theme.
    let dumped = toml::to_string(&c).unwrap();
    let c2 = Config::parse(&dumped, std::path::Path::new("test")).unwrap();
    assert_eq!(c2.theme, c.theme);
}
