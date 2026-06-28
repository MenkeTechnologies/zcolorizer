//! Modern log-format modules — the formats you actually tail today that ccze
//! (2003) never covered: systemd/journald, sshd/auth, kernel, nginx, JSON &
//! logfmt, Postgres/MySQL/Redis/Mongo, Docker/Kubernetes, app frameworks, and
//! common network services. Each was researched against real on-disk lines.
//!
//! Same model as [`crate::modules`]: a [`Module`] is a named bundle of
//! [`RuleDef`]s whose named capture groups are tokens. Enabled via
//! `-m <name>` / `modules = [...]` / `-m all`.

use crate::modules::Module;
use crate::rules::RuleDef;

/// All modern-format modules, in a stable order.
pub fn all() -> Vec<Module> {
    vec![
        // structured / generic
        json(),
        logfmt(),
        // system / init
        journald(),
        authlog(),
        kernel(),
        cron(),
        auditd(),
        // web / proxy
        nginx(),
        haproxy(),
        caddy(),
        traefik(),
        // databases
        postgres(),
        mysql(),
        redis(),
        mongodb(),
        // containers / orchestration
        klog(),
        docker(),
        // app frameworks
        spring(),
        python(),
        rust_log(),
        rails(),
        // network / mail / security
        named(),
        dnsmasq(),
        dhcpd(),
        fail2ban(),
        ufw(),
        dovecot(),
    ]
}

// ===========================================================================
// structured / generic
// ===========================================================================

/// json — generic structured/JSON application logs (one JSON object per line),
/// as emitted by zap, logrus(JSONFormatter), bunyan, pino, slog, winston.
fn json() -> Module {
    Module::new(
        "json",
        "Structured JSON application logs (zap/logrus/bunyan/pino/slog)",
        vec![
            RuleDef::new("json-key", r#"(?P<json_key>"[\w.\-]+")\s*:"#),
            RuleDef::new(
                "json-level-error",
                r#""(?:level|severity|lvl|levelname|loglevel)"\s*:\s*(?P<error>"(?i:error|err|fatal|crit|critical|alert|emerg|emergency|panic)")"#,
            ),
            RuleDef::new(
                "json-level-warning",
                r#""(?:level|severity|lvl|levelname|loglevel)"\s*:\s*(?P<warning>"(?i:warn|warning)")"#,
            ),
            RuleDef::new(
                "json-level-info",
                r#""(?:level|severity|lvl|levelname|loglevel)"\s*:\s*(?P<info>"(?i:info|notice|log)")"#,
            ),
            RuleDef::new(
                "json-level-debug",
                r#""(?:level|severity|lvl|levelname|loglevel)"\s*:\s*(?P<debug>"(?i:debug|trace|fine)")"#,
            ),
            RuleDef::new("json-true", r"(?P<good>\btrue\b)"),
            RuleDef::new("json-false", r"(?P<bad>\bfalse\b)"),
            RuleDef::new("json-null", r"(?P<unknown>\bnull\b)"),
            RuleDef::new("json-number", r":\s*(?P<number>-?\d+(?:\.\d+)?(?:[eE][+\-]?\d+)?)"),
            RuleDef::new("json-string-value", r#":\s*(?P<string>"(?:[^"\\]|\\.)*")"#),
        ],
    )
}

/// logfmt — key=value structured logs (Go kit/log, Heroku, Grafana, many Go svcs).
fn logfmt() -> Module {
    Module::new(
        "logfmt",
        "logfmt key=value structured logs (Go kit/log, Heroku, Grafana)",
        vec![
            RuleDef::new("logfmt-date", r"=(?P<date>\d{4}-\d{2}-\d{2}T[0-9:.+\-Z]+)"),
            RuleDef::new("logfmt-duration", r"=(?P<duration>\d+(?:\.\d+)?(?:ns|us|µs|ms|s|m|h))\b"),
            RuleDef::new(
                "logfmt-level-error",
                r"(?:level|lvl|severity)=(?P<error>(?i:error|err|fatal|crit|critical|alert|emerg|emergency|panic))\b",
            ),
            RuleDef::new("logfmt-level-warning", r"(?:level|lvl|severity)=(?P<warning>(?i:warn|warning))\b"),
            RuleDef::new("logfmt-level-info", r"(?:level|lvl|severity)=(?P<info>(?i:info|notice|log))\b"),
            RuleDef::new("logfmt-level-debug", r"(?:level|lvl|severity)=(?P<debug>(?i:debug|trace|fine))\b"),
            RuleDef::new("logfmt-string-value", r#"=(?P<string>"(?:[^"\\]|\\.)*")"#),
            RuleDef::new("logfmt-number", r"=(?P<number>-?\d+(?:\.\d+)?)\b"),
            RuleDef::new("logfmt-key", r"(?P<field>[A-Za-z_][\w.\-]*)="),
        ],
    )
}

// ===========================================================================
// system / init
// ===========================================================================

/// journald — `journalctl`/`journalctl -f` "short" output (syslog-style) plus
/// systemd unit state-transition messages (Started/Stopped/Failed).
fn journald() -> Module {
    Module::new(
        "journald",
        "systemd journal (journalctl short output) + unit state messages",
        vec![
            RuleDef::new(
                "journald-line",
                r"^(?P<date>\w{3}\s{1,2}\d{1,2}\s\d\d:\d\d:\d\d)\s(?P<host>\S+)\s(?P<process>[\w.@/\\-]+)(?:\[(?P<pid>\d+)\])?:",
            ),
            RuleDef::new(
                "journald-unit",
                r"\b(?P<service>[\w.@\\-]+\.(?:service|socket|target|mount|timer|scope|slice|path|device|automount|swap))\b",
            ),
            RuleDef::with_token("journald-start", r"\b(?:Started|Starting|Reached target|Listening on|Mounted|Activated|Created slice|Server listening)\b", "good"),
            RuleDef::with_token("journald-stop", r"\b(?:Stopped|Stopping|Unmounted|Deactivated successfully|Removed slice|Closed|Reloading|Reloaded)\b", "info"),
            RuleDef::with_token("journald-fail", r"(?:Failed to start|Failed with result|entered failed state|start-limit-hit|Main process exited|Killing process|Watchdog timeout|core-dump|Dependency failed)", "error").ci(),
        ],
    )
}

/// authlog — /var/log/auth.log and /var/log/secure: sshd, sudo, PAM events.
fn authlog() -> Module {
    Module::new(
        "authlog",
        "SSH/sudo/PAM authentication log (auth.log, secure)",
        vec![
            RuleDef::new(
                "authlog-line",
                r"^(?P<date>\w{3}\s{1,2}\d{1,2}\s\d\d:\d\d:\d\d)\s(?P<host>\S+)\s(?P<process>[\w./-]+)(?:\[(?P<pid>\d+)\])?:",
            ),
            RuleDef::new("authlog-for-user", r"\bfor (?:invalid user |illegal user )?(?P<user>\S+) from\b"),
            RuleDef::new("authlog-pam-user", r"\bfor user (?P<user>\S+)\b"),
            RuleDef::new("authlog-sudo-user", r"\bUSER=(?P<user>\S+)\b"),
            RuleDef::with_token("authlog-ok", r"\b(?:Accepted (?:password|publickey|keyboard-interactive(?:/pam)?|gssapi-with-mic)|session opened|New session)\b", "good"),
            RuleDef::with_token("authlog-info", r"\b(?:session closed|Disconnected from|Connection closed by|Removed session)\b", "info"),
            RuleDef::with_token("authlog-bad", r"(?:Failed password|Failed publickey|authentication failure|Invalid user|Illegal user|Connection reset by|maximum authentication attempts exceeded|not allowed because|Did not receive identification string|Bad protocol version identification|POSSIBLE BREAK-IN ATTEMPT|Too many authentication failures)", "bad"),
            RuleDef::with_token("authlog-sudo-fail", r"(?:incorrect password attempts|authentication failure;|user NOT in sudoers|command not allowed)", "error"),
        ],
    )
}

/// kernel — dmesg raw output and /var/log/kern.log.
fn kernel() -> Module {
    Module::new(
        "kernel",
        "Kernel ring buffer (dmesg) and kern.log",
        vec![
            RuleDef::new(
                "kernel-syslog",
                r"^(?P<date>\w{3}\s{1,2}\d{1,2}\s\d\d:\d\d:\d\d)\s(?P<host>\S+)\s(?P<process>kernel):",
            ),
            RuleDef::new("kernel-ts", r"\[\s*(?P<time>\d+\.\d+)\]"),
            RuleDef::with_token("kernel-error", r"(?:Kernel panic|Oops|BUG:|general protection fault|segfault|Call Trace|Out of memory|killed process|I/O error|EXT4-fs error|hung task|soft lockup|hard LOCKUP|unable to handle|not syncing|machine check|stack-protector)", "error").ci(),
            RuleDef::with_token("kernel-warn", r"(?:WARNING:|deprecated|firmware bug|reset(?:ting)?|timed out|retrying|disabling|throttled|tainted|over-?current|failed to)", "warning").ci(),
        ],
    )
}

/// cron — cron/crond/anacron syslog entries: "(user) ACTION (detail)".
fn cron() -> Module {
    Module::new(
        "cron",
        "cron/crond/anacron syslog entries",
        vec![
            RuleDef::new(
                "cron-line",
                r"^(?P<date>\w{3}\s{1,2}\d{1,2}\s\d\d:\d\d:\d\d)\s(?P<host>\S+)\s(?P<process>(?:CRON|cron|crond|anacron))(?:\[(?P<pid>\d+)\])?:",
            ),
            RuleDef::new(
                "cron-action",
                r"\((?P<user>[\w.-]+)\)\s(?P<keyword>CMD|MAIL|RELOAD|LIST|FINISHED|INFO|REPLACE|DELETE|STARTUP|WRONG|ERROR|BAD)\b",
            ),
            RuleDef::with_token("cron-error", r"(?:ERROR|BAD FILE MODE|cannot|No MTA installed|Permission denied|unable to|orphan|bad minute|bad hour)", "error"),
        ],
    )
}

/// auditd — /var/log/audit/audit.log: "type=NAME msg=audit(...): key=value ...".
fn auditd() -> Module {
    Module::new(
        "auditd",
        "Linux audit daemon log (audit.log)",
        vec![
            RuleDef::new(
                "auditd-header",
                r"^type=(?P<keyword>\w+)\smsg=audit\((?P<time>\d+\.\d+):(?P<number>\d+)\):",
            ),
            RuleDef::with_token("auditd-ok", r"\b(?:success=yes|res=success|result=Success)\b", "good"),
            RuleDef::with_token("auditd-fail", r"\b(?:success=no|res=failed|result=Failed)\b", "bad"),
            RuleDef::with_token("auditd-strval", r#""[^"]*""#, "string"),
            RuleDef::new("auditd-key", r"\b(?P<field>\w+)="),
        ],
    )
}

// ===========================================================================
// web / proxy
// ===========================================================================

/// nginx — access (combined) and error logs.
fn nginx() -> Module {
    Module::new(
        "nginx",
        "Coloriser for nginx access (combined) and error logs",
        vec![
            RuleDef::new(
                "nginx-access",
                r#"^(?P<ip>\S+)\s+\S+\s+(?P<user>\S+)\s+\[(?P<date>[^\]]+)\]\s+"(?P<http_method>[A-Z]+)\s+(?P<uri>\S+)\s+(?P<protocol>HTTP/[\d.]+)"\s+(?P<http_code>\d{3})\s+(?P<size>\d+|-)"#,
            ),
            RuleDef::new(
                "nginx-error",
                r"^(?P<date>\d{4}/\d\d/\d\d)\s+(?P<time>\d\d:\d\d:\d\d)\s+\[(?:debug|info|notice|warn|error|crit|alert|emerg)\]\s+(?P<pid>\d+)#(?P<thread>\d+):(?:\s+\*(?P<unique>\d+))?",
            ),
            RuleDef::with_token("nginx-lvl-error", r"\[(?:emerg|alert|crit|error)\]", "error"),
            RuleDef::with_token("nginx-lvl-warn", r"\[warn\]", "warning"),
            RuleDef::with_token("nginx-lvl-info", r"\[(?:notice|info)\]", "info"),
            RuleDef::with_token("nginx-lvl-debug", r"\[debug\]", "debug"),
        ],
    )
}

/// haproxy — HAProxy HTTP log (option httplog), via syslog.
fn haproxy() -> Module {
    Module::new(
        "haproxy",
        "Coloriser for HAProxy HTTP (httplog) access logs",
        vec![
            RuleDef::new(
                "haproxy-line",
                r"^(?P<date>\w{3}\s+\d{1,2}\s\d\d:\d\d:\d\d)\s+(?:(?P<host>\S+)\s+)?(?P<process>haproxy)\[(?P<pid>\d+)\]:\s+(?P<ip>\d{1,3}(?:\.\d{1,3}){3}):\d+\s+\[(?P<time>[^\]]+)\]\s+(?P<service>\S+\s+\S+)\s+(?P<duration>[\d/+-]+)\s+(?P<http_code>\d{3})\s+(?P<size>[\d-]+)",
            ),
            RuleDef::new(
                "haproxy-request",
                r#""(?P<http_method>[A-Z]+)\s+(?P<uri>\S+)\s+(?P<protocol>HTTP/[\d.]+)""#,
            ),
        ],
    )
}

/// caddy — Caddy v2 default JSON access log.
fn caddy() -> Module {
    Module::new(
        "caddy",
        "Coloriser for Caddy v2 default JSON access logs",
        vec![
            RuleDef::with_token("caddy-lvl-error", r#""level"\s*:\s*"(?:error|fatal|panic)""#, "error"),
            RuleDef::with_token("caddy-lvl-warn", r#""level"\s*:\s*"warn""#, "warning"),
            RuleDef::with_token("caddy-lvl-info", r#""level"\s*:\s*"info""#, "info"),
            RuleDef::with_token("caddy-lvl-debug", r#""level"\s*:\s*"debug""#, "debug"),
            RuleDef::new("caddy-ts", r#""ts"\s*:\s*(?P<time>[\d.]+)"#),
            RuleDef::new("caddy-status", r#""status"\s*:\s*(?P<http_code>\d+)"#),
            RuleDef::new("caddy-duration", r#""duration"\s*:\s*(?P<duration>[\d.]+)"#),
            RuleDef::new("caddy-size", r#""(?:size|bytes_read)"\s*:\s*(?P<size>\d+)"#),
            RuleDef::new("caddy-method", r#""method"\s*:\s*"(?P<http_method>[A-Z]+)""#),
            RuleDef::new("caddy-uri", r#""uri"\s*:\s*"(?P<uri>[^"]*)""#),
            RuleDef::new("caddy-ip", r#""(?:remote_ip|client_ip)"\s*:\s*"(?P<ip>[^"]*)""#),
            RuleDef::with_token("caddy-true", r"\btrue\b", "good"),
            RuleDef::with_token("caddy-false", r"\bfalse\b", "bad"),
            RuleDef::with_token("caddy-null", r"\bnull\b", "unknown"),
            RuleDef::with_token("caddy-key", r#""[\w.]+"\s*:"#, "json_key"),
            RuleDef::new("caddy-number", r#":\s*(?P<number>-?\d+(?:\.\d+)?)"#),
            RuleDef::with_token("caddy-string", r#""[^"]*""#, "string"),
        ],
    )
}

/// traefik — Traefik access log, default "common" (extended CLF) format.
fn traefik() -> Module {
    Module::new(
        "traefik",
        "Coloriser for Traefik access logs (extended CLF)",
        vec![RuleDef::new(
            "traefik-line",
            r#"^(?P<ip>\S+)\s+\S+\s+(?P<user>\S+)\s+\[(?P<date>[^\]]+)\]\s+"(?P<http_method>[A-Z]+)\s+(?P<uri>\S+)\s+(?P<protocol>HTTP/[\d.]+)"\s+(?P<http_code>\d{3})\s+(?P<size>\d+)\s+"[^"]*"\s+"[^"]*"\s+(?P<number>\d+)\s+"(?P<service>[^"]*)"\s+"(?P<address>[^"]*)"\s+(?P<duration>\d+(?:\.\d+)?)ms"#,
        )],
    )
}

// ===========================================================================
// databases
// ===========================================================================

/// postgres — PostgreSQL server log (default log_line_prefix '%m [%p] ').
fn postgres() -> Module {
    Module::new(
        "postgres",
        "Coloriser for PostgreSQL server logs (stderr / logging_collector)",
        vec![
            RuleDef::new(
                "postgres-line",
                r"^(?P<date>\d{4}-\d{2}-\d{2})\s(?P<time>\d{2}:\d{2}:\d{2}\.\d+(?:\s\w+)?)\s\[(?P<pid>\d+)\]",
            ),
            RuleDef::with_token("postgres-error", r"\b(?:FATAL|PANIC|ERROR):", "error"),
            RuleDef::with_token("postgres-warning", r"\bWARNING:", "warning"),
            RuleDef::with_token("postgres-info", r"\b(?:LOG|STATEMENT):", "info"),
            RuleDef::with_token("postgres-debug", r"\b(?:DETAIL|HINT|CONTEXT|NOTICE):", "debug"),
        ],
    )
}

/// mysql — MySQL 8.x / MariaDB error log (traditional text format).
fn mysql() -> Module {
    Module::new(
        "mysql",
        "Coloriser for MySQL / MariaDB error logs",
        vec![
            RuleDef::new(
                "mysql-line",
                r"^(?P<date>\d{4}-\d{2}-\d{2})T(?P<time>\d{2}:\d{2}:\d{2}\.\d+Z?)\s+(?P<thread>\d+)\s+\[(?:System|Warning|Error|Note)\]\s+\[(?P<unique>MY-\d+)\]\s+\[(?P<service>\w+)\]",
            ),
            RuleDef::new(
                "mysql-maria-line",
                r"^(?P<date>\d{4}-\d{2}-\d{2})\s(?P<time>\d{2}:\d{2}:\d{2})\s+(?P<thread>\d+)\s+\[(?:System|Warning|Error|Note)\]",
            ),
            RuleDef::with_token("mysql-error", r"\[Error\]", "error").ci(),
            RuleDef::with_token("mysql-warning", r"\[Warning\]", "warning").ci(),
            RuleDef::with_token("mysql-info", r"\[(?:System|Note)\]", "info").ci(),
        ],
    )
}

/// redis — Redis / Valkey server log (text format).
/// "1234:M 02 Jan 2024 15:04:05.123 * message"; level char . - * #.
fn redis() -> Module {
    Module::new(
        "redis",
        "Coloriser for Redis / Valkey server logs",
        vec![
            RuleDef::new(
                "redis-line",
                r"^(?P<pid>\d+):(?P<process>[MSCX])\s(?P<date>\d{1,2}\s\w{3}\s\d{4})\s(?P<time>\d{2}:\d{2}:\d{2}\.\d+)",
            ),
            RuleDef::new("redis-warning", r"^\d+:[MSCX]\s\d{1,2}\s\w{3}\s\d{4}\s\d{2}:\d{2}:\d{2}\.\d+\s(?P<warning>#)"),
            RuleDef::new("redis-info", r"^\d+:[MSCX]\s\d{1,2}\s\w{3}\s\d{4}\s\d{2}:\d{2}:\d{2}\.\d+\s(?P<info>[*\-])"),
            RuleDef::new("redis-debug", r"^\d+:[MSCX]\s\d{1,2}\s\w{3}\s\d{4}\s\d{2}:\d{2}:\d{2}\.\d+\s(?P<debug>\.)"),
        ],
    )
}

/// mongodb — MongoDB 4.4+ structured JSON log. Specific ($date, severity) rules
/// run BEFORE the generic string/number rules so they win the span.
fn mongodb() -> Module {
    Module::new(
        "mongodb",
        "Coloriser for MongoDB 4.4+ structured JSON logs",
        vec![
            RuleDef::new("mongodb-key", r#"(?P<json_key>"[\$\w]+")\s*:"#),
            RuleDef::new("mongodb-date", r#""\$date"\s*:\s*(?P<date>"[^"]+")"#),
            RuleDef::new("mongodb-sev-error", r#""s"\s*:\s*(?P<error>"[FE]")"#),
            RuleDef::new("mongodb-sev-warning", r#""s"\s*:\s*(?P<warning>"W")"#),
            RuleDef::new("mongodb-sev-info", r#""s"\s*:\s*(?P<info>"I")"#),
            RuleDef::new("mongodb-sev-debug", r#""s"\s*:\s*(?P<debug>"D\d?")"#),
            RuleDef::new("mongodb-bool-true", r"(?P<good>\btrue\b)"),
            RuleDef::new("mongodb-bool-false", r"(?P<bad>\bfalse\b)"),
            RuleDef::new("mongodb-string", r#":\s*(?P<string>"[^"]*")"#),
            RuleDef::new("mongodb-number", r#":\s*(?P<number>-?\d+(?:\.\d+)?)"#),
        ],
    )
}

// ===========================================================================
// containers / orchestration
// ===========================================================================

/// klog — Kubernetes/glog leveled logs: "[IWEF]mmdd hh:mm:ss.uuuuuu tid file:line] msg".
fn klog() -> Module {
    Module::new(
        "klog",
        "Kubernetes/glog klog leveled logs",
        vec![
            RuleDef::new("klog-info", r"^(?P<info>I)(?P<date>\d{4})\s(?P<time>\d{2}:\d{2}:\d{2}\.\d+)\s+(?P<pid>\d+)\s+(?P<file>\S+\.go:\d+)\]"),
            RuleDef::new("klog-warning", r"^(?P<warning>W)(?P<date>\d{4})\s(?P<time>\d{2}:\d{2}:\d{2}\.\d+)\s+(?P<pid>\d+)\s+(?P<file>\S+\.go:\d+)\]"),
            RuleDef::new("klog-error", r"^(?P<error>E)(?P<date>\d{4})\s(?P<time>\d{2}:\d{2}:\d{2}\.\d+)\s+(?P<pid>\d+)\s+(?P<file>\S+\.go:\d+)\]"),
            RuleDef::new("klog-fatal", r"^(?P<error>F)(?P<date>\d{4})\s(?P<time>\d{2}:\d{2}:\d{2}\.\d+)\s+(?P<pid>\d+)\s+(?P<file>\S+\.go:\d+)\]"),
        ],
    )
}

/// docker — Docker daemon (logrus text) and json-file container logs.
fn docker() -> Module {
    Module::new(
        "docker",
        "Docker daemon (logrus text) and json-file container logs",
        vec![
            RuleDef::new("docker-time", r#"^time="(?P<date>[^"]+)""#),
            RuleDef::new("docker-level-error", r"\blevel=(?P<error>(?i:error|fatal|panic))\b"),
            RuleDef::new("docker-level-warning", r"\blevel=(?P<warning>(?i:warn|warning))\b"),
            RuleDef::new("docker-level-info", r"\blevel=(?P<info>(?i:info))\b"),
            RuleDef::new("docker-level-debug", r"\blevel=(?P<debug>(?i:debug|trace))\b"),
            RuleDef::new("docker-string-value", r#"=(?P<string>"(?:[^"\\]|\\.)*")"#),
            RuleDef::new("docker-key", r"(?P<field>[A-Za-z_][\w.\-]*)="),
            RuleDef::new("docker-json-key", r#"(?P<json_key>"[\w.\-]+")\s*:"#),
            RuleDef::new("docker-stream", r#""stream"\s*:\s*(?P<string>"std(?:out|err)")"#),
            RuleDef::new("docker-json-time", r#""time"\s*:\s*"(?P<date>[^"]+)""#),
        ],
    )
}

// ===========================================================================
// app frameworks
// ===========================================================================

/// spring — Spring Boot default Logback console pattern.
fn spring() -> Module {
    Module::new(
        "spring",
        "Spring Boot / Logback default console log coloriser",
        vec![
            RuleDef::new(
                "spring-line",
                r"^(?P<date>\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3})\s+(?:ERROR|WARN|INFO|DEBUG|TRACE)\s+(?P<pid>\d+)\s+---\s+\[(?P<thread>[^\]]*)\]\s+(?P<process>\S+)\s+:",
            ),
            RuleDef::with_token("spring-lvl-error", r"\bERROR\b", "error"),
            RuleDef::with_token("spring-lvl-warn", r"\bWARN\b", "warning"),
            RuleDef::with_token("spring-lvl-info", r"\bINFO\b", "info"),
            RuleDef::with_token("spring-lvl-debug", r"\b(?:DEBUG|TRACE)\b", "debug"),
        ],
    )
}

/// python — stdlib `logging` output (asctime / basicConfig / [LEVEL] variants).
fn python() -> Module {
    Module::new(
        "python",
        "Python stdlib logging output coloriser",
        vec![
            RuleDef::new(
                "python-line",
                r"^(?P<date>\d{4}-\d\d-\d\d \d\d:\d\d:\d\d,\d{3})\s-\s(?P<process>[\w.]+)\s-\s(?:DEBUG|INFO|WARNING|ERROR|CRITICAL)\s-\s",
            ),
            RuleDef::new("python-colon-line", r"^(?:DEBUG|INFO|WARNING|ERROR|CRITICAL):(?P<process>[\w.]+):"),
            RuleDef::new("python-bracket-line", r"^\[(?:DEBUG|INFO|WARNING|ERROR|CRITICAL)\]"),
            RuleDef::with_token("python-lvl-error", r"\b(?:ERROR|CRITICAL)\b", "error"),
            RuleDef::with_token("python-lvl-warn", r"\bWARNING\b", "warning"),
            RuleDef::with_token("python-lvl-info", r"\bINFO\b", "info"),
            RuleDef::with_token("python-lvl-debug", r"\bDEBUG\b", "debug"),
        ],
    )
}

/// rust_log — env_logger and tracing-subscriber fmt output.
fn rust_log() -> Module {
    Module::new(
        "rust-log",
        "Rust env_logger / tracing-subscriber fmt coloriser",
        vec![
            RuleDef::new(
                "rust-log-env",
                r"^\[(?P<date>\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ)\s+(?:ERROR|WARN|INFO|DEBUG|TRACE)\s+(?P<process>[\w:]+)\]",
            ),
            RuleDef::new(
                "rust-log-tracing",
                r"^(?P<date>\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z)\s+(?:ERROR|WARN|INFO|DEBUG|TRACE)\s+(?P<process>[\w:]+):",
            ),
            RuleDef::with_token("rust-log-error", r"\bERROR\b", "error"),
            RuleDef::with_token("rust-log-warn", r"\bWARN\b", "warning"),
            RuleDef::with_token("rust-log-info", r"\bINFO\b", "info"),
            RuleDef::with_token("rust-log-debug", r"\b(?:DEBUG|TRACE)\b", "debug"),
        ],
    )
}

/// rails — Ruby/Rails default Logger::Formatter.
fn rails() -> Module {
    Module::new(
        "rails",
        "Ruby / Rails default Logger::Formatter coloriser",
        vec![
            RuleDef::new(
                "rails-line",
                r"^[DIWEFU], \[(?P<date>\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d+) #(?P<pid>\d+)\]\s+(?:DEBUG|INFO|WARN|ERROR|FATAL|UNKNOWN)\s+--\s+(?P<process>\S*)\s*:",
            ),
            RuleDef::with_token("rails-sev-error", r"^[EF],", "error"),
            RuleDef::with_token("rails-sev-warn", r"^W,", "warning"),
            RuleDef::with_token("rails-sev-info", r"^I,", "info"),
            RuleDef::with_token("rails-sev-debug", r"^D,", "debug"),
            RuleDef::with_token("rails-lvl-error", r"\b(?:ERROR|FATAL)\b", "error"),
            RuleDef::with_token("rails-lvl-warn", r"\bWARN\b", "warning"),
            RuleDef::with_token("rails-lvl-info", r"\bINFO\b", "info"),
            RuleDef::with_token("rails-lvl-debug", r"\bDEBUG\b", "debug"),
        ],
    )
}

// ===========================================================================
// network / mail / security
// ===========================================================================

/// named — BIND 9 named query log (DD-Mon-YYYY date, queries category).
fn named() -> Module {
    Module::new(
        "named",
        "BIND 9 named query log",
        vec![
            RuleDef::new(
                "named-line",
                r"^(?P<date>\d{2}-[A-Z][a-z]{2}-\d{4})\s+(?P<time>\d{2}:\d{2}:\d{2}\.\d{3})\s+(?P<facility>[a-z-]+):\s+(?:info|notice|warning|error|debug|critical):\s+client(?:\s+@0x[0-9a-f]+)?\s+(?P<ip>[0-9a-fA-F:.]+)#(?P<number>\d+)\s+\((?P<host>[^)]+)\):\s+query:",
            ),
            RuleDef::with_token("named-client", r"@0x[0-9a-f]+", "unique"),
            RuleDef::with_token(
                "named-rrtype",
                r"\b(?:IN|CH|HS)\s+(?:A|AAAA|PTR|MX|NS|CNAME|SOA|TXT|SRV|SPF|CAA|NAPTR|DNSKEY|RRSIG|NSEC3?|DS|TLSA|HTTPS|SVCB|ANY)\b",
                "protocol",
            ),
        ],
    )
}

/// dnsmasq — DNS/DHCP server, logs to syslog (process dnsmasq / dnsmasq-dhcp).
fn dnsmasq() -> Module {
    Module::new(
        "dnsmasq",
        "dnsmasq DNS query/reply log (syslog)",
        vec![
            RuleDef::new("dnsmasq-proc", r"\b(?P<process>dnsmasq(?:-dhcp)?)\[(?P<pid>\d+)\]:"),
            RuleDef::new("dnsmasq-query", r"\b(?P<keyword>query)\[(?P<protocol>[A-Z0-9]+)\]\s+(?P<host>\S+)"),
            RuleDef::with_token(
                "dnsmasq-verb",
                r"\b(?:reply|reply-truncated|cached|cached-stale|forwarded|config|local|DHCPDISCOVER|DHCPOFFER|DHCPREQUEST|DHCPACK|DHCPNAK)\b",
                "keyword",
            ),
        ],
    )
}

/// dhcpd — ISC DHCP server, logs to syslog.
fn dhcpd() -> Module {
    Module::new(
        "dhcpd",
        "ISC dhcpd lease-protocol log (syslog)",
        vec![
            RuleDef::new("dhcpd-proc", r"\b(?P<process>dhcpd)\[(?P<pid>\d+)\]:"),
            RuleDef::with_token(
                "dhcpd-msg",
                r"\bDHCP(?:DISCOVER|OFFER|REQUEST|ACK|NAK|INFORM|DECLINE|RELEASE|EXPIRE|LEASEQUERY)\b",
                "keyword",
            ),
            RuleDef::new("dhcpd-via", r"\bvia\s+(?P<service>[\w.:-]+)"),
        ],
    )
}

/// fail2ban — fail2ban.actions / fail2ban.filter logger (fail2ban.log).
fn fail2ban() -> Module {
    Module::new(
        "fail2ban",
        "fail2ban server log",
        vec![
            RuleDef::new(
                "fail2ban-line",
                r"^(?P<date>\d{4}-\d{2}-\d{2})\s+(?P<time>\d{2}:\d{2}:\d{2},\d{3})\s+(?P<process>fail2ban\.[\w.-]+)\s+\[(?P<pid>\d+)\]:\s+(?:INFO|NOTICE|WARNING|ERROR|CRITICAL)\s+\[(?P<service>[^\]]+)\]",
            ),
            RuleDef::with_token("fail2ban-ban", r"\bBan\b", "bad"),
            RuleDef::with_token("fail2ban-unban", r"\bUnban\b", "good"),
            RuleDef::with_token("fail2ban-found", r"\bFound\b", "warning"),
            RuleDef::with_token("fail2ban-restore", r"\bRestore Ban\b", "warning"),
            RuleDef::with_token("fail2ban-already", r"\balready banned\b", "warning").ci(),
        ],
    )
}

/// ufw — kernel netfilter log lines emitted by ufw ([UFW ...] KEY=VALUE).
fn ufw() -> Module {
    Module::new(
        "ufw",
        "ufw/netfilter packet-filter log",
        vec![
            RuleDef::with_token("ufw-limit", r"\[UFW LIMIT BLOCK\]", "warning"),
            RuleDef::with_token("ufw-block", r"\[UFW BLOCK\]", "bad"),
            RuleDef::with_token("ufw-allow", r"\[UFW ALLOW\]", "good"),
            RuleDef::with_token("ufw-audit", r"\[UFW AUDIT(?: INVALID)?\]", "info"),
            RuleDef::new("ufw-proto", r"\bPROTO=(?P<protocol>\w+)"),
            RuleDef::new("ufw-key", r"\b(?P<field>[A-Z][A-Z0-9]{1,6})="),
        ],
    )
}

/// dovecot — IMAP/POP3/LMTP server log.
fn dovecot() -> Module {
    Module::new(
        "dovecot",
        "Dovecot IMAP/POP3/LMTP log",
        vec![
            RuleDef::new(
                "dovecot-svc",
                r"\b(?P<service>imap|pop3|lmtp|managesieve|submission|auth|doveadm|imap-login|pop3-login|submission-login)(?:\((?P<user>[^)]*)\))?(?:<(?P<pid>\d+)>)?(?:<(?P<hash>[^>]+)>)?:",
            ),
            RuleDef::new("dovecot-kv", r"\b(?P<field>[a-z_]+)="),
            RuleDef::with_token("dovecot-login", r"\bLogin\b", "good"),
            RuleDef::with_token("dovecot-logout", r"\b(?:Logged out|Disconnected)\b", "info"),
            RuleDef::with_token(
                "dovecot-authfail",
                r"\b(?:auth failed|Authentication failed|Aborted login|disconnected \(auth failed)\b",
                "bad",
            )
            .ci(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::compile_all;

    #[test]
    fn all_modern_modules_compile() {
        for m in all() {
            compile_all(&m.rules)
                .unwrap_or_else(|e| panic!("modern module `{}` has a bad rule: {e}", m.name));
        }
    }

    #[test]
    fn count_is_27() {
        assert_eq!(all().len(), 27);
    }
}
