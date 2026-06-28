//! Format-specific colorizers — the port of ccze's plugins (`mod_*.c`).
//!
//! In ccze each plugin owns one or more whole-line PCRE regexes; on a match it
//! paints the numbered capture groups with semantic colors and hands the
//! free-text remainder to the word-colorizer. We model a plugin as a [`Module`]:
//! a named bundle of [`RuleDef`]s whose **named capture groups are tokens**. When
//! a module's structured fields claim their spans, the generic rules
//! ([`crate::rules::builtin_generic`]) color whatever is left — exactly ccze's
//! module-then-wordcolor flow.
//!
//! Modules are opt-in via `--module <name>` (repeatable) or `modules = [...]` in
//! config; with none selected, only the generic ruleset runs. `--module all`
//! enables every module (specific patterns are anchored with `^`, so they only
//! fire on lines that genuinely match that format).
//!
//! ## Translation notes (ccze C → RuleDef)
//!
//! * A ccze numbered group `N` colored `CCZE_COLOR_X` becomes a named group
//!   `(?P<x>...)`. Separators ccze prints in `default`/with `ccze_space()` are
//!   simply left uncaptured (they render in the default style).
//! * Fields ccze leaves for the word-colorizer (e.g. the syslog message) are
//!   *not* captured, so the generic rules pick them up.
//! * Where ccze chooses a color dynamically by substring (squid proxy
//!   action/hierarchy, HTTP method), we emit small follow-up whole-match rules
//!   for each variant token instead of one group.

use crate::rules::RuleDef;

/// A named, self-contained colorizer for one log format.
#[derive(Debug, Clone)]
pub struct Module {
    pub name: &'static str,
    pub description: &'static str,
    pub rules: Vec<RuleDef>,
}

impl Module {
    fn new(name: &'static str, description: &'static str, rules: Vec<RuleDef>) -> Module {
        Module { name, description, rules }
    }
}

/// Every ported module, in a stable order. `--module all` enables all of these.
pub fn all() -> Vec<Module> {
    vec![
        syslog(),
        httpd(),
        squid(),
        // The remaining ports are appended by `extra_modules()` so the big block
        // of translated definitions stays in one place.
    ]
    .into_iter()
    .chain(extra_modules())
    .collect()
}

/// Look up a module by name (case-insensitive).
pub fn get(name: &str) -> Option<Module> {
    all().into_iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// Resolve a list of requested module names into their rule defs, in order.
/// `"all"` expands to every module. Unknown names are returned in the `Err`.
pub fn resolve(names: &[String]) -> Result<Vec<RuleDef>, Vec<String>> {
    let mut out = Vec::new();
    let mut unknown = Vec::new();
    for name in names {
        if name.eq_ignore_ascii_case("all") {
            for m in all() {
                out.extend(m.rules);
            }
            continue;
        }
        match get(name) {
            Some(m) => out.extend(m.rules),
            None => unknown.push(name.clone()),
        }
    }
    if unknown.is_empty() {
        Ok(out)
    } else {
        Err(unknown)
    }
}

// ===========================================================================
// Reference ports (template for the rest). Patterns are anchored with `^` so a
// module only fires on lines in its own format.
// ===========================================================================

/// `mod_syslog.c` — `Mon DD HH:MM:SS host proc[pid]: message`.
/// Captures the date/host/process/pid prefix; the message is left for the
/// generic word-colorizer (matching ccze, which passes the rest to wordcolor).
fn syslog() -> Module {
    Module::new(
        "syslog",
        "Generic syslog(8) log coloriser",
        vec![
            // Date + host + "proc[pid]:" prefix. The message after ": " is not
            // captured on purpose so generic rules color it.
            RuleDef::new(
                "syslog-line",
                r"^(?P<date>\S+\s{1,2}\d{1,2}\s\d\d:\d\d:\d\d)\s(?P<host>\S+)\s+(?P<process>[\w./-]+)(?:\[(?P<pid>\d+)\])?:",
            ),
            // "last message repeated N times" / "-- MARK --" → repeat token.
            RuleDef::with_token(
                "syslog-repeat",
                r"(?:last message repeated \d+ times|-- MARK --)",
                crate::theme::tokens::REPEAT,
            ),
        ],
    )
}

/// `mod_httpd.c` — Apache/nginx access (combined/common) and error logs.
fn httpd() -> Module {
    use crate::theme::tokens::*;
    Module::new(
        "httpd",
        "Coloriser for generic HTTPD access and error logs",
        vec![
            // Access log (common/combined): host - user [date] "METHOD path proto" code size ...
            // ccze groups: vhost? host user date action method code gsize other.
            RuleDef::new(
                "httpd-access",
                r#"^(?P<host>\S+)\s+(?:\S+\s+)?-\s+(?P<user>\S+)\s+(?P<date>\[[^\]]+\])\s+"(?P<http_method>[A-Z]+)[^"]*"\s+(?P<http_code>\d{3})\s+(?P<getsize>\d+|-)"#,
            ),
            // Error log: [Day Mon DD HH:MM:SS YYYY] [level] message
            RuleDef::new(
                "httpd-error",
                r"^(?P<date>\[\w{3}\s\w{3}\s{1,2}\d{1,2}\s\d{2}:\d{2}:\d{2}\s\d{4}\])\s+(?P<level>\[\w+\])",
            ),
            // Color the error-log level word inside its brackets.
            RuleDef::with_token("httpd-lvl-error", r"\[(?:error|crit|alert|emerg)\]", ERROR).ci(),
            RuleDef::with_token("httpd-lvl-warn", r"\[warn\]", WARNING).ci(),
            RuleDef::with_token("httpd-lvl-debug", r"\[(?:debug|info|notice)\]", DEBUG).ci(),
        ],
    )
}

/// `mod_squid.c` — squid access, store and cache logs.
fn squid() -> Module {
    use crate::theme::tokens::*;
    Module::new(
        "squid",
        "Coloriser for squid access, store and cache logs",
        vec![
            // access.log: time elapsed host action/code size method uri ident hierarchy/from ctype
            // The action (TCP_MISS…) and hierarchy (DIRECT…) words and the forward
            // host are matched non-capturing so the dynamic follow-up rules below
            // (and the generic ip/host rules) color them instead of the base.
            RuleDef::new(
                "squid-access",
                r"^(?P<time>\d{9,10}\.\d{3})\s+(?P<gettime>\d+)\s(?P<host>\S+)\s\w+/(?P<http_code>\d{3})\s(?P<getsize>\d+)\s(?P<http_method>\w+)\s(?P<uri>\S+)\s(?P<ident>\S+)\s\w+/(?:[\d.]+|-)\s(?P<ctype>\S+)",
            ),
            // cache.log: YYYY/MM/DD HH:MM:SS| message
            RuleDef::new("squid-cache", r"^(?P<date>\d{4}/\d{2}/\d{2}\s(?:\d{2}:){2}\d{2})\|"),
            // Proxy action result codes (TCP_HIT, TCP_MISS, …) — dynamic color by substring.
            RuleDef::with_token("squid-hit", r"\b\w*HIT\w*\b", PROXY_HIT),
            RuleDef::with_token("squid-miss", r"\b\w*MISS\w*\b", PROXY_MISS),
            RuleDef::with_token("squid-denied", r"\b\w*DENIED\w*\b", PROXY_DENIED),
            RuleDef::with_token("squid-refresh", r"\b\w*REFRESH\w*\b", PROXY_REFRESH),
            RuleDef::with_token("squid-swapfail", r"\b\w*SWAPFAIL\w*\b", PROXY_SWAPFAIL),
            RuleDef::with_token("squid-direct", r"\bDIRECT\b", PROXY_DIRECT),
            RuleDef::with_token("squid-parent", r"\bPARENT\b", PROXY_PARENT),
            RuleDef::with_token("squid-err", r"\bERR_\w+\b", ERROR),
        ],
    )
}

// ===========================================================================
// The remaining ccze plugin ports (translated from vendor/ccze/src/mod_*.c).
// ===========================================================================

fn extra_modules() -> Vec<Module> {
    vec![
        dpkg(),
        postfix(),
        exim(),
        procmail(),
        proftpd(),
        vsftpd(),
        ftpstats(),
        xferlog(),
        php(),
        oops(),
        icecast(),
        fetchmail(),
        apm(),
        distcc(),
        sulog(),
        super_(),
        ulogd(),
    ]
}

/// `mod_dpkg.c` — Debian dpkg package-manager log lines (status/action/conffile).
fn dpkg() -> Module {
    Module::new(
        "dpkg",
        "Coloriser for dpkg logs.",
        vec![
            // YYYY-MM-DD HH:MM:SS status <state> <pkg> <installed-version>
            RuleDef::new(
                "dpkg-status",
                r"^(?P<date>[-\d]{10}\s[:\d]{8})\sstatus\s(?P<pkgstatus>\S+)\s(?P<pkg>\S+)\s\S+$",
            ),
            // YYYY-MM-DD HH:MM:SS <action> <pkg> <installed-version> <available-version>
            RuleDef::new(
                "dpkg-action",
                r"^(?P<date>[-\d]{10}\s[:\d]{8})\s(?:install|upgrade|remove|purge)\s(?P<pkg>\S+)\s\S+\s\S+$",
            ),
            // YYYY-MM-DD HH:MM:SS conffile <filename> <decision>
            RuleDef::new(
                "dpkg-conffile",
                r"^(?P<date>[-\d]{10}\s[:\d]{8})\sconffile\s(?P<file>\S+)\s(?:install|keep)$",
            ),
            RuleDef::with_token(
                "dpkg-keyword",
                r"\b(?:status|conffile|install|upgrade|remove|purge|keep)\b",
                "keyword",
            ),
        ],
    )
}

/// `mod_postfix.c` — Postfix queue sub-log lines (`<spoolid>: field=value, ...`).
fn postfix() -> Module {
    Module::new(
        "postfix",
        "Coloriser for postfix(1) sub-logs.",
        vec![
            RuleDef::new(
                "postfix-line",
                r"^(?P<unique>[\dA-F]+): (?P<field>client|to|message-id|uid|resent-message-id|from)=",
            ),
        ],
    )
}

/// `mod_exim.c` — Exim MTA log lines (date, 16-char message id, action arrow, message).
fn exim() -> Module {
    Module::new(
        "exim",
        "Coloriser for exim logs.",
        vec![
            RuleDef::new(
                "exim-action",
                r"^(?P<date>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2})\s(?P<unique>\S{16})\s(?:[<=*][=>*])\s",
            ),
            RuleDef::new(
                "exim-uniqn",
                r"^(?P<date>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2})\s(?P<unique>\S{16})\s",
            ),
            RuleDef::new(
                "exim-date",
                r"^(?P<date>\d{4}-\d{2}-\d{2}\s\d{2}:\d{2}:\d{2})\s",
            ),
            // Action arrows: "<=" incoming, "=>"/"->" outgoing, "**"/"==" error.
            RuleDef::with_token("exim-incoming", r"<[=>*]", "incoming"),
            RuleDef::with_token("exim-outgoing", r"[=*]>", "outgoing"),
            RuleDef::with_token("exim-error", r"[=*][=*]", "error"),
        ],
    )
}

/// `mod_procmail.c` — procmail log lines (From / Subject: / Folder: headers).
fn procmail() -> Module {
    Module::new(
        "procmail",
        "Coloriser for procmail(1) logs.",
        vec![
            RuleDef::new(
                "procmail-from",
                r"^\s*(?P<field>>?From)\s(?P<email>\S+)\s+(?P<date>.*)$",
            )
            .ci(),
            RuleDef::new("procmail-subject", r"^\s*(?P<field>Subject:)\s(?P<subject>\S+)").ci(),
            RuleDef::new(
                "procmail-folder",
                r"^\s*(?P<field>Folder:)\s(?P<dir>\S+)\s+(?P<size>.*)$",
            )
            .ci(),
        ],
    )
}

/// `mod_proftpd.c` — proftpd access and auth log lines (common-log-style FTP).
fn proftpd() -> Module {
    Module::new(
        "proftpd",
        "Coloriser for proftpd access and auth logs.",
        vec![
            RuleDef::new(
                "proftpd-access",
                r#"^(?P<host>\d+\.\d+\.\d+\.\d+) (?P<ident>\S+) (?P<user>\S+) \[(?P<date>\d{2}/.{3}/\d{4}:\d{2}:\d{2}:\d{2} [\-+]\d{4})\] "(?P<keyword>[A-Z]+) (?P<uri>[^"]+)" (?P<ftp_code>\d{3}) (?P<getsize>-|\d+)"#,
            ),
            RuleDef::new(
                "proftpd-auth",
                r#"^(?P<host>\S+) ftp server \[(?P<pid>\d+)\] (?P<ip>\d+\.\d+\.\d+\.\d+) \[(?P<date>\d{2}/.{3}/\d{4}:\d{2}:\d{2}:\d{2} [\-+]\d{4})\] "(?P<keyword>[A-Z]+) (?:[^"]+)" (?P<ftp_code>\d{3})"#,
            ),
        ],
    )
}

/// `mod_vsftpd.c` — vsftpd(8) session log lines with `[pid N]` and optional `[user]`.
fn vsftpd() -> Module {
    Module::new(
        "vsftpd",
        "Coloriser for vsftpd(8) logs.",
        vec![
            RuleDef::new(
                "vsftpd-line",
                r"^(?P<date>\S+\s+\S+\s+\d{1,2}\s+\d{1,2}:\d{1,2}:\d{1,2}\s+\d+)\s+\[pid (?P<pid>\d+)\]\s+(?:\[(?P<user>\S+)\])?\s*",
            ),
        ],
    )
}

/// `mod_ftpstats.c` — pure-ftpd ftpstats transfer records.
fn ftpstats() -> Module {
    Module::new(
        "ftpstats",
        "Coloriser for ftpstats (pure-ftpd) logs.",
        vec![
            RuleDef::new(
                "ftpstats-line",
                r"^(?P<date>\d{9,10})\s(?P<unique>[\da-f]+\.[\da-f]+)\s(?P<user>\S+)\s(?P<host>\S+)\s(?P<ftp_code>U|D)\s(?P<getsize>\d+)\s(?P<gettime>\d+)\s(?P<dir>.*)$",
            ),
        ],
    )
}

/// `mod_xferlog.c` — generic wu-ftpd/pure-ftpd xferlog transfer records.
fn xferlog() -> Module {
    Module::new(
        "xferlog",
        "Generic xferlog coloriser.",
        vec![
            RuleDef::new(
                "xferlog-line",
                r"^(?P<date>... ... +\d{1,2} +\d{1,2}:\d{1,2}:\d{1,2} \d+) (?P<gettime>\d+) (?P<host>[^ ]+) (?P<getsize>\d+) (?P<dir>\S+) (?P<bracket>a|b) (?P<ftp_code>C|U|T|_) (?:o|i) (?:a|g|r) (?P<user>[^ ]+) (?P<service>[^ ]+) (?:0|1) (?P<ident>[^ ]+) (?:c|i)",
            ),
        ],
    )
}

/// `mod_php.c` — PHP error log lines of the form `[date] PHP <message>`.
fn php() -> Module {
    Module::new(
        "php",
        "Coloriser for PHP logs.",
        vec![
            RuleDef::new("php-line", r"^(?P<date>\[\d+-\w{3}-\d+ \d+:\d+:\d+\]) PHP "),
            RuleDef::with_token("php-keyword", r"\bPHP\b", "keyword"),
            RuleDef::with_token("php-error", r"\b(?:fatal error|parse error|error)\b", "error").ci(),
            RuleDef::with_token("php-warning", r"\bwarning\b", "warning").ci(),
            RuleDef::with_token("php-notice", r"\b(?:notice|deprecated)\b", "debug").ci(),
        ],
    )
}

/// `mod_oops.c` — oops proxy `statistics()` lines: date `[id]statistics(): field : value`.
fn oops() -> Module {
    Module::new(
        "oops",
        "Coloriser for oops proxy logs.",
        vec![
            RuleDef::new(
                "oops-line",
                r"^(?P<date>(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun) (?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) \d+ \d+:\d+:\d+ \d+)\s+\[(?P<process>[\dxa-fA-F]+)\]statistics\(\): (?P<field>\S+)\s*: (?P<number>\d+)",
            ),
            RuleDef::with_token("oops-keyword", r"statistics\(\)", "keyword"),
        ],
    )
}

/// `mod_icecast.c` — Icecast access logs and bandwidth-usage summary lines.
fn icecast() -> Module {
    Module::new(
        "icecast",
        "Coloriser for Icecast(8) logs.",
        vec![
            RuleDef::new(
                "icecast-usage",
                r"^(?P<date>\[\d+/\w{3}/\d+:\d+:\d+:\d+\]) \[(?P<pid>\d+):(?P<keyword>[^\]]*)\] \[\d+/\w{3}/\d+:\d+:\d+:\d+\] Bandwidth:(?P<size>[\d.]+)\S* Sources:(?P<number>\d+) Clients:\d+ Admins:\d+",
            ),
            RuleDef::new(
                "icecast-line",
                r"^(?P<date>\[\d+/\w{3}/\d+:\d+:\d+:\d+\]) (?:(?P<keyword>Admin) )?\[(?P<pid>\d*):?(?P<host>[^\]]*)\] ",
            ),
            RuleDef::with_token("icecast-label", r"\b(?:Bandwidth|Sources|Clients|Admins):", "keyword"),
            RuleDef::with_token("icecast-date", r"\[\d+/\w{3}/\d+:\d+:\d+:\d+\]", "date"),
        ],
    )
}

/// `mod_fetchmail.c` — fetchmail "reading message <addr>:N of M" progress lines.
fn fetchmail() -> Module {
    Module::new(
        "fetchmail",
        "Coloriser for fetchmail(1) sub-logs.",
        vec![
            RuleDef::new(
                "fetchmail-line",
                r"reading message (?P<email>[^@]*@[^:]*):(?P<number>\d+) of (?P<size>\d+)",
            ),
        ],
    )
}

/// `mod_apm.c` — APM (battery/power) status sub-logs.
fn apm() -> Module {
    Module::new(
        "apm",
        "Coloriser for APM sub-logs.",
        // The two percentages and two HH:MM:SS times reuse a color; since the Rust
        // regex crate forbids duplicate group names in one pattern, the line is
        // split into single-field rules. The trailing message is left uncaptured.
        vec![
            RuleDef::new("apm-battery", r"Battery: (?P<percentage>-?\d+)%"),
            RuleDef::new("apm-charge", r"%, (?P<system>.*charging) \("),
            RuleDef::new("apm-rate", r"charging \((?P<percentage>-?\d+)%"),
            RuleDef::new("apm-elapsed", r"% \S* (?P<date>\d+:\d+:\d+)\),"),
            RuleDef::new("apm-remain", r"\), (?P<date>\d+:\d+:\d+) "),
        ],
    )
}

/// `mod_distcc.c` — distcc(1) distributed-compiler daemon logs.
fn distcc() -> Module {
    Module::new(
        "distcc",
        "Coloriser for distcc(1) logs.",
        vec![RuleDef::new(
            "distcc-line",
            r"^(?P<process>distccd)\[(?P<pid>\d+)\] (?P<keyword>\([^)]+\))?",
        )],
    )
}

/// `mod_sulog.c` — su(1) login sulog records.
fn sulog() -> Module {
    Module::new(
        "sulog",
        "Coloriser for su(1) logs.",
        vec![
            RuleDef::new(
                "sulog-tty-unknown",
                r"^SU \d{2}/\d{2} \d{2}:\d{2} [+-] (?P<unknown>\?\S*) ",
            ),
            RuleDef::new(
                "sulog-line",
                r"^SU (?P<date>\d{2}/\d{2} \d{2}:\d{2}) [+-] (?P<dir>\S+) (?P<user>[^-]+)-",
            ),
            RuleDef::new(
                "sulog-touser",
                r"^SU \d{2}/\d{2} \d{2}:\d{2} [+-] \S+ [^-]+-(?P<user>.*)$",
            ),
        ],
    )
}

/// `mod_super.c` — super(1) privileged-command logs. (`super` is a Rust keyword.)
fn super_() -> Module {
    Module::new(
        "super",
        "Coloriser for super(1) logs.",
        vec![RuleDef::new(
            "super-line",
            r"^(?P<email>\S+)\s(?P<date>\w+\s+\w+\s+\d+\s+\d+:\d+:\d+\s+\d+)\s+(?P<process>\S+)\s\([^)]+\)",
        )],
    )
}

/// `mod_ulogd.c` — ulogd / iptables packet-log `field=value` sub-logs.
fn ulogd() -> Module {
    Module::new(
        "ulogd",
        "Coloriser for ulogd sub-logs.",
        // Capture each "FIELD=" name; values stay uncaptured for the generic rules
        // (ip, mac, number, …).
        vec![RuleDef::new("ulogd-field", r"(?P<field>[A-Za-z][A-Za-z0-9]*)=")],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::compile_all;

    #[test]
    fn all_modules_compile() {
        for m in all() {
            compile_all(&m.rules)
                .unwrap_or_else(|e| panic!("module `{}` has a bad rule: {e}", m.name));
        }
    }

    #[test]
    fn resolve_all_keyword() {
        let defs = resolve(&["all".to_string()]).unwrap();
        assert!(!defs.is_empty());
    }

    #[test]
    fn resolve_unknown_reports() {
        let err = resolve(&["nope".to_string()]).unwrap_err();
        assert_eq!(err, vec!["nope".to_string()]);
    }

    #[test]
    fn syslog_module_exists() {
        assert!(get("syslog").is_some());
    }
}
