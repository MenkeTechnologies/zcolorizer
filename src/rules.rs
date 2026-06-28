//! Rules: regex patterns that tag spans of a line with a semantic token.
//!
//! Two flavors, both expressed as one [`RuleDef`]:
//!
//! * **Named-group rules** — the regex has named capture groups and each group
//!   name *is* the token, e.g. `(?P<date>\d{4}-\d\d-\d\d)\s+(?P<host>\S+)`. One
//!   regex can paint several differently-colored fields. This is the pygments
//!   "regex → token" model and the recommended way to write config rules.
//! * **Whole-match rules** — no named groups; the entire match gets `token`
//!   (defaulting to `default`). Handy for word lists.
//!
//! Rules are tried in order; earlier rules own a span and later ones can't
//! recolor bytes already claimed (see [`crate::engine`]).

use crate::theme::tokens;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Serializable rule definition (what lives in the config file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleDef {
    /// Human-readable label (shown in `--list-rules`, errors).
    #[serde(default)]
    pub name: String,
    /// The regex. Named capture groups map to tokens of the same name.
    pub pattern: String,
    /// Token for a whole-match (no named groups) rule. Defaults to `default`.
    #[serde(default)]
    pub token: Option<String>,
    /// Case-insensitive matching (compiles with `(?i)`).
    #[serde(default)]
    pub ignore_case: bool,
}

impl RuleDef {
    pub fn new(name: &str, pattern: &str) -> RuleDef {
        RuleDef { name: name.into(), pattern: pattern.into(), token: None, ignore_case: false }
    }
    pub fn with_token(name: &str, pattern: &str, token: &str) -> RuleDef {
        RuleDef {
            name: name.into(),
            pattern: pattern.into(),
            token: Some(token.into()),
            ignore_case: false,
        }
    }
    pub fn ci(mut self) -> RuleDef {
        self.ignore_case = true;
        self
    }
}

/// A compiled rule ready to match.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub regex: Regex,
    /// Token applied to a whole match when the regex has no named groups.
    pub whole_token: String,
    /// Token name for each named capture group, in `regex.capture_names()` order
    /// (index 0 is the implicit whole match and is `None`).
    pub group_tokens: Vec<Option<String>>,
    pub has_named_groups: bool,
}

impl Rule {
    /// Compile a [`RuleDef`]. Returns the regex error with the rule name for context.
    pub fn compile(def: &RuleDef) -> Result<Rule, crate::error::Error> {
        let pat = if def.ignore_case {
            format!("(?i){}", def.pattern)
        } else {
            def.pattern.clone()
        };
        let regex = Regex::new(&pat).map_err(|e| crate::error::Error::BadRule {
            name: def.name.clone(),
            source: e,
        })?;
        let group_tokens: Vec<Option<String>> =
            regex.capture_names().map(|n| n.map(|s| s.to_string())).collect();
        let has_named_groups = group_tokens.iter().skip(1).any(|n| n.is_some());
        Ok(Rule {
            name: def.name.clone(),
            regex,
            whole_token: def.token.clone().unwrap_or_else(|| tokens::DEFAULT.to_string()),
            group_tokens,
            has_named_groups,
        })
    }
}

/// Compile a list of rule definitions, surfacing the first bad pattern.
pub fn compile_all(defs: &[RuleDef]) -> Result<Vec<Rule>, crate::error::Error> {
    defs.iter().map(Rule::compile).collect()
}

/// The builtin generic ruleset — a port of ccze's word-coloriser plus a handful of
/// structured-line rules. These run when the config doesn't replace them and give a
/// reasonable colorization for arbitrary logs out of the box.
#[allow(clippy::vec_init_then_push)] // grouped, commented pushes read clearer than one giant vec![]
pub fn builtin_generic() -> Vec<RuleDef> {
    use tokens::*;
    let mut r = Vec::new();

    // ---- Structured leading timestamps (named-group, so multiple fields at once) ----
    // ISO 8601: 2026-06-27T14:03:11.123Z  /  2026-06-27 14:03:11
    r.push(RuleDef::new(
        "iso-datetime",
        r"(?P<date>\d{4}-\d{2}-\d{2})[T ](?P<time>\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)",
    ));
    // Syslog: Jun 27 14:03:11
    r.push(RuleDef::new(
        "syslog-datetime",
        r"(?P<date>(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+\d{1,2})\s+(?P<time>\d{2}:\d{2}:\d{2})",
    ));
    // proc[pid]: — process name + bracketed pid
    r.push(RuleDef::new("proc-pid", r"(?P<process>[\w./-]+)\[(?P<pid>\d+)\]"));

    // ---- Log levels / severity keywords (whole-match word rules) ----
    r.push(RuleDef::with_token("lvl-error", r"\b(ERROR|ERR|CRITICAL|CRIT|FATAL|EMERG|ALERT|PANIC)\b", ERROR).ci());
    r.push(RuleDef::with_token("lvl-warn", r"\b(WARNING|WARN)\b", WARNING).ci());
    r.push(RuleDef::with_token("lvl-debug", r"\b(DEBUG|TRACE)\b", DEBUG).ci());
    r.push(RuleDef::with_token("lvl-info", r"\b(INFO|NOTICE)\b", INFO).ci());

    // ---- ccze "good"/"bad"/"system" word lists ----
    r.push(RuleDef::with_token(
        "bad-words",
        r"\b(warn\w*|restart\w*|exit\w*|stop\w*|shut\w*|down|close\w*|unreach\w*|can'?t|cannot|skip\w*|den\w+|disabl\w+|ignor\w+|miss\w*|oops|fail\w*|unable|readonly|offline|terminat\w*|empty|virus|reject\w*|refus\w+|timed?\s?out|timeout)\b",
        BAD,
    ).ci());
    r.push(RuleDef::with_token(
        "good-words",
        r"\b(activ\w*|start\w*|read\w+|online|loaded|ok|success\w*|register\w*|detected|configured|enabl\w+|listen\w*|open\w*|complete\w*|done|connect\w*|finish\w*|clean|accept\w*|established|up)\b",
        GOOD,
    ).ci());
    r.push(RuleDef::with_token(
        "system-words",
        r"\b(ext[234]|reiserfs|xfs|btrfs|zfs|vfs|iso9?6?6?0?|isofs|ppp|bsd|linux|tcp/ip|mtrr|pci\w*|isa|scsi|ide|atapi|bios|cpu|fpu|kernel|systemd|udev|dbus)\b",
        SYSTEM,
    ).ci());

    // ---- HTTP ----
    r.push(RuleDef::with_token("http-method", r"\b(GET|POST|PUT|DELETE|HEAD|OPTIONS|PATCH|CONNECT|TRACE)\b", HTTP_METHOD));
    // Status code in a combined/common access-log line: `… HTTP/1.1" 200 1234`.
    // Named group so only the code is painted; anchored to `HTTP/x.x"` to avoid
    // coloring every 3-digit number as a status code.
    r.push(RuleDef::new("http-code", r#"HTTP/\d\.\d"?\s+(?P<http_code>[1-5]\d{2})\b"#));

    // ---- Network / identifiers ----
    r.push(RuleDef::with_token("ipv4", r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b", IP));
    r.push(RuleDef::with_token("ipv6", r"\b(?:[0-9a-fA-F]{1,4}:){2,7}[0-9a-fA-F]{1,4}\b", IP));
    r.push(RuleDef::with_token("mac", r"\b(?:[0-9a-fA-F]{2}:){5}[0-9a-fA-F]{2}\b", MAC));
    r.push(RuleDef::with_token("email", r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b", EMAIL));
    r.push(RuleDef::with_token("uri", r#"\b[a-z][a-z0-9+.-]*://[^\s)\]}>'"]+"#, URI).ci());
    r.push(RuleDef::with_token("hostname", r"\b(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)+[a-z]{2,}\b", HOST).ci());

    // ---- Filesystem ----
    r.push(RuleDef::with_token("abs-path", r"(?:^|\s)(?P<dir>/[\w./@+-]*)", DIR));

    // ---- Numbers / sizes / versions / addresses ----
    r.push(RuleDef::with_token("hex-addr", r"\b0x[0-9a-fA-F]+\b", ADDRESS));
    r.push(RuleDef::with_token("size", r"\b\d+(?:\.\d+)?\s?(?:[KMGTP]i?B|[kmgtp]b?|bytes?)\b", SIZE));
    r.push(RuleDef::with_token("version", r"\bv?\d+\.\d+(?:\.\d+)*(?:[-+][\w.]+)?\b", VERSION).ci());
    r.push(RuleDef::with_token("percentage", r"\b\d+(?:\.\d+)?%", PERCENTAGE));
    r.push(RuleDef::with_token("signal", r"\bSIG(?:HUP|INT|QUIT|ILL|TRAP|ABRT|BUS|FPE|KILL|USR1|SEGV|USR2|PIPE|ALRM|TERM|CHLD|CONT|STOP|TSTP|TTIN|TTOU)\b", SIGNAL));
    r.push(RuleDef::with_token("number", r"\b\d+\b", NUMBER));

    // ---- Quoted strings ----
    r.push(RuleDef::with_token("dq-string", r#""[^"]*""#, STRING));

    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_ruleset_compiles() {
        let rules = compile_all(&builtin_generic()).expect("all builtin rules compile");
        assert!(!rules.is_empty());
    }

    #[test]
    fn named_groups_detected() {
        let rule = Rule::compile(&RuleDef::new("dt", r"(?P<date>\d+)-(?P<time>\d+)")).unwrap();
        assert!(rule.has_named_groups);
        assert_eq!(rule.group_tokens, vec![None, Some("date".into()), Some("time".into())]);
    }

    #[test]
    fn whole_match_default_token() {
        let rule = Rule::compile(&RuleDef::with_token("e", r"ERROR", "error")).unwrap();
        assert!(!rule.has_named_groups);
        assert_eq!(rule.whole_token, "error");
    }
}
