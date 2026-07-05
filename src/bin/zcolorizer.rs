//! `zcolorizer` — pipe logs in, get cyberpunk-colored logs out.
//!
//! ```text
//! tail -f /var/log/syslog | zcolorizer
//! zcolorizer --theme ccze-classic access.log
//! journalctl -f | zcolorizer -t cyberpunk
//! ```

use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime};

use clap::Parser;
use regex::Regex;
use zcolorizer::config::Config;
use zcolorizer::Colorizer;

/// How many leading lines `-m auto` samples to fingerprint the log format.
const SNIFF_LINES: usize = 200;
/// Minimum wall-clock gap between config-mtime checks while `--watch`ing, so a
/// busy stream doesn't `stat(2)` on every single line.
const RELOAD_THROTTLE: Duration = Duration::from_millis(200);

#[derive(Parser, Debug)]
#[command(
    name = "zcolorizer",
    version,
    about = "Real-time log colorizer — fully customizable rules, swappable themes (cyberpunk by default)",
    long_about = None,
    // House-style cyberpunk help is rendered by `print_cyberpunk_help`, not clap.
    disable_help_flag = true,
)]
struct Cli {
    /// Print this help.
    #[arg(short = 'h', long = "help")]
    help: bool,

    /// Theme to use (overrides the config). See --list-themes.
    #[arg(short, long)]
    theme: Option<String>,

    /// Enable a format module (ccze plugin port): syslog, httpd, squid, … or `all`.
    /// Use `auto` to sniff the input and enable only the formats actually present.
    /// Repeatable. Merged with any `modules` in the config.
    #[arg(short = 'm', long = "module")]
    modules: Vec<String>,

    /// Config file (default: $XDG_CONFIG_HOME/zcolorizer/config.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Re-read the config (theme, rules, modules) when the file changes on disk,
    /// while streaming. Pairs with the zgui theme picker for live recoloring.
    #[arg(short = 'w', long)]
    watch: bool,

    /// Show only lines matching this regex (still colorized). Like `grep`, but
    /// keeps color. Inline flags work, e.g. `(?i)error`.
    #[arg(short = 'g', long, value_name = "REGEX")]
    grep: Option<String>,

    /// Spotlight: dim the lines that DON'T match this regex; matches stay at full
    /// color. Keeps surrounding context visible instead of dropping it.
    #[arg(short = 'H', long, value_name = "REGEX")]
    highlight: Option<String>,

    /// Input files to colorize. With none, reads stdin.
    files: Vec<PathBuf>,

    /// Force color even when stdout is not a TTY (e.g. piping into a pager).
    #[arg(short = 'C', long)]
    force_color: bool,

    /// Never emit color (passthrough). Useful to sanity-check input.
    #[arg(long)]
    no_color: bool,

    /// List available themes and exit.
    #[arg(long)]
    list_themes: bool,

    /// List available format modules and exit.
    #[arg(long)]
    list_modules: bool,

    /// Emit all resolved themes as JSON (for tooling) and exit.
    #[arg(long)]
    themes_json: bool,

    /// List the effective rules (after config merge) and exit.
    #[arg(long)]
    list_rules: bool,

    /// Print the resolved config as TOML and exit.
    #[arg(long)]
    dump_config: bool,
}

/// The ANSI-Shadow "ZCOLORIZER" wordmark (matches the README banner).
const BANNER: &str = r#"███████╗ ██████╗ ██████╗ ██╗      ██████╗ ██████╗ ██╗███████╗███████╗██████╗
╚══███╔╝██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗██║╚══███╔╝██╔════╝██╔══██╗
  ███╔╝ ██║     ██║   ██║██║     ██║   ██║██████╔╝██║  ███╔╝ █████╗  ██████╔╝
 ███╔╝  ██║     ██║   ██║██║     ██║   ██║██╔══██╗██║ ███╔╝  ██╔══╝  ██╔══██╗
███████╗╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║██║███████╗███████╗██║  ██║
╚══════╝ ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝╚══════╝╚══════╝╚═╝  ╚═╝"#;

/// Render the cyberpunk house-style `-h` help (banner + cyan section rules +
/// green `//` descriptions), matching the rest of the MenkeTechnologies CLI suite.
fn print_cyberpunk_help() {
    let bin = env!("CARGO_BIN_NAME");
    let version = env!("CARGO_PKG_VERSION");

    const C: &str = "\x1b[36m"; // cyan — section rules
    const M: &str = "\x1b[35m"; // magenta
    const Y: &str = "\x1b[33m"; // yellow — USAGE
    const G: &str = "\x1b[32m"; // green — `//` comment marker
    const D: &str = "\x1b[2m"; // dim — footer
    const N: &str = "\x1b[0m"; // reset

    // Banner with a vertical cyan→magenta neon gradient (truecolor).
    let lines: Vec<&str> = BANNER.lines().collect();
    let last = (lines.len().saturating_sub(1)).max(1) as f32;
    for (i, line) in lines.iter().enumerate() {
        let t = i as f32 / last;
        let r = (0.0 + t * 255.0) as u8;
        let g = (229.0 - t * 186.0) as u8;
        let b = (255.0 - t * 41.0) as u8;
        println!("\x1b[1;38;2;{r};{g};{b}m{line}{N}");
    }
    let module_count = zcolorizer::modules::all().len();
    println!();
    println!("  {M}Real-time log colorizer{N} — ccze/pygments port · 31 cyberpunk themes · {module_count} modules");
    println!();

    println!("{Y}  USAGE:{N} {bin} [OPTIONS] [FILES]...        {G}//{N} reads stdin when no FILES");
    println!("         tail -f /var/log/syslog | {bin} -m syslog");
    println!();

    let row = |flags: &str, desc: &str| println!("  {flags:<24}{G}//{N} {desc}");

    println!("{C}  ── INPUT ─────────────────────────────────────────────{N}");
    row(
        "FILES...",
        "files to colorize (default: stdin, line-buffered)",
    );
    println!();

    println!("{C}  ── THEME ─────────────────────────────────────────────{N}");
    row(
        "-t, --theme NAME",
        "theme to use (default: neon-sprawl, alias cyberpunk)",
    );
    row(
        "    --list-themes",
        "list all themes (active marked with *)",
    );
    row(
        "    --themes-json",
        "emit every theme as JSON (for tooling)",
    );
    println!();

    println!("{C}  ── RULES & MODULES ───────────────────────────────────{N}");
    row(
        "-m, --module NAME",
        "enable a format module (repeatable; `all`, `auto`)",
    );
    row(
        "    --list-modules",
        &format!("list the {module_count} format modules"),
    );
    row(
        "    --list-rules",
        "list effective rules after config merge",
    );
    row(
        "-c, --config PATH",
        "config file (default ~/.config/zcolorizer/config.toml)",
    );
    row(
        "-w, --watch",
        "reload config/theme live when the file changes",
    );
    row("    --dump-config", "print the resolved config as TOML");
    println!();

    println!("{C}  ── OUTPUT ────────────────────────────────────────────{N}");
    row(
        "-g, --grep REGEX",
        "show only matching lines (still colorized)",
    );
    row(
        "-H, --highlight REGEX",
        "spotlight: dim the lines that don't match",
    );
    row("-C, --force-color", "color even when stdout is not a TTY");
    row("    --no-color", "never color (passthrough)");
    println!();

    println!("{C}  ── INFO ──────────────────────────────────────────────{N}");
    row("-h, --help", "print this help");
    row("-V, --version", "print version");
    println!();

    println!("{D}  zcolorizer v{version} · MenkeTechnologies · cyberpunk by default{N}");
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("zcolorizer: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> zcolorizer::Result<ExitCode> {
    if cli.help {
        print_cyberpunk_help();
        return Ok(ExitCode::SUCCESS);
    }

    let mut config = match &cli.config {
        Some(p) => Config::load(p)?,
        None => Config::load_default()?,
    };

    // `auto` is a meta-module: it means "sniff the input and enable the formats
    // actually present". It can't be resolved here (we haven't read input yet),
    // so pull it out of the real module lists and remember it.
    let auto = cli
        .modules
        .iter()
        .chain(config.modules.iter())
        .any(|m| is_auto(m));
    config.modules.retain(|m| !is_auto(m));

    // Modules requested on the CLI (minus `auto`). Kept separately so `--watch`
    // can re-apply them on top of a freshly-loaded config after each reload.
    let cli_modules: Vec<String> = cli
        .modules
        .iter()
        .filter(|m| !is_auto(m))
        .cloned()
        .collect();

    // Merge CLI-requested modules into the config (dedup, preserve order).
    for m in &cli_modules {
        if !config.modules.iter().any(|x| x.eq_ignore_ascii_case(m)) {
            config.modules.push(m.clone());
        }
    }

    if cli.list_modules {
        for m in zcolorizer::modules::all() {
            println!("{:<12} {}", m.name, m.description);
        }
        return Ok(ExitCode::SUCCESS);
    }

    if cli.list_themes {
        // Canonical name of the active theme (resolves aliases/display names).
        let active = config
            .resolve_theme(cli.theme.as_deref())
            .map(|t| t.name)
            .unwrap_or_default();
        for name in config.available_theme_names() {
            let marker = if name == active { "*" } else { " " };
            println!("{marker} {name}");
        }
        return Ok(ExitCode::SUCCESS);
    }

    if cli.themes_json {
        // Resolve every available theme to a full Theme struct so the picker can
        // render real swatches from each token's style.
        let themes: Vec<_> = config
            .available_theme_names()
            .into_iter()
            .filter_map(|n| config.resolve_theme(Some(&n)).ok())
            .collect();
        let active = config
            .resolve_theme(cli.theme.as_deref())
            .map(|t| t.name)
            .unwrap_or_default();
        let doc = serde_json::json!({ "active": active, "themes": themes });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return Ok(ExitCode::SUCCESS);
    }

    if cli.list_rules {
        for def in config.resolve_rule_defs() {
            let tok = def.token.as_deref().unwrap_or("(named groups)");
            println!("{:<22} {:<14} {}", def.name, tok, def.pattern);
        }
        return Ok(ExitCode::SUCCESS);
    }

    if cli.dump_config {
        // Re-emit a normalized config so zgui can round-trip it.
        let s = toml::to_string_pretty(&config).unwrap_or_default();
        print!("{s}");
        return Ok(ExitCode::SUCCESS);
    }

    // Compile the optional grep/highlight filters (reusing the crate's rule-error
    // type so a bad pattern is reported the same way a bad rule is).
    let grep = compile_filter("--grep", cli.grep.as_deref())?;
    let highlight = compile_filter("--highlight", cli.highlight.as_deref())?;

    let stdout = io::stdout();
    let want_color = cli.force_color || (!cli.no_color && stdout.is_terminal());

    // `-m auto`: sniff a sample of the input, then enable the detected modules.
    // For stdin we must buffer the sniffed lines and replay them; for files we
    // sample the head and re-open them for the real pass.
    let mut sniffed: Vec<Vec<u8>> = Vec::new();
    let mut stdin_reader: Option<BufReader<io::StdinLock<'static>>> = None;
    let mut extra_modules = cli_modules;
    if auto {
        let detected: Vec<String> = if cli.files.is_empty() {
            let mut reader = BufReader::new(io::stdin().lock());
            sniffed =
                sniff_reader(&mut reader, SNIFF_LINES).map_err(|e| sniff_err("<stdin>", e))?;
            let lines: Vec<String> = sniffed.iter().map(|r| line_body(r).into_owned()).collect();
            stdin_reader = Some(reader);
            detect_names(&lines)
        } else {
            detect_names(&sniff_files(&cli.files, SNIFF_LINES).map_err(|e| sniff_err("input", e))?)
        };
        eprintln!(
            "zcolorizer: -m auto detected: {}",
            if detected.is_empty() {
                "(none)".to_string()
            } else {
                detected.join(", ")
            }
        );
        for d in detected {
            if !config.modules.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
                config.modules.push(d.clone());
            }
            if !extra_modules.iter().any(|x| x.eq_ignore_ascii_case(&d)) {
                extra_modules.push(d);
            }
        }
    }

    let unknown = config.unknown_modules();
    if !unknown.is_empty() {
        eprintln!(
            "zcolorizer: unknown module(s): {} (see --list-modules)",
            unknown.join(", ")
        );
    }

    let colorizer = Colorizer::from_config(&config, cli.theme.as_deref())?;

    // The file we re-read on `--watch`: the explicit `-c` path, or the default
    // path if it actually exists.
    let watch_path = cli
        .config
        .clone()
        .or_else(|| Config::default_path().filter(|p| p.exists()));
    let reload = if cli.watch {
        match watch_path {
            Some(p) => Some(Reloader::new(p, cli.theme.clone(), extra_modules)),
            None => {
                eprintln!("zcolorizer: --watch: no config file to watch; ignoring");
                None
            }
        }
    } else {
        None
    };

    let mut session = Session {
        colorizer,
        want_color,
        grep,
        highlight,
        reload,
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    let result = if cli.files.is_empty() {
        let reader = stdin_reader.unwrap_or_else(|| BufReader::new(io::stdin().lock()));
        session
            .replay(&sniffed, &mut out)
            .and_then(|()| session.run_reader(reader, &mut out))
    } else {
        let mut last = Ok(());
        for path in &cli.files {
            match std::fs::File::open(path) {
                Ok(f) => {
                    if let Err(e) = session.run_reader(BufReader::new(f), &mut out) {
                        last = Err(e);
                    }
                }
                Err(e) => {
                    let _ = out.flush();
                    eprintln!("zcolorizer: {}: {e}", path.display());
                    last = Err(e);
                }
            }
        }
        last
    };

    let _ = out.flush();
    match result {
        Ok(()) => Ok(ExitCode::SUCCESS),
        // A broken pipe (downstream pager quit) is normal, not an error.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(ExitCode::SUCCESS),
        Err(e) => {
            eprintln!("zcolorizer: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// True if a module name is the `auto` meta-module (case-insensitive).
fn is_auto(name: &str) -> bool {
    name.eq_ignore_ascii_case("auto")
}

/// Run format detection and own the resulting names as `String`s.
fn detect_names(sample: &[String]) -> Vec<String> {
    zcolorizer::modules::detect(sample)
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Wrap an I/O error from the `-m auto` sniff phase as a crate error.
fn sniff_err(what: &str, source: io::Error) -> zcolorizer::Error {
    zcolorizer::Error::Io {
        path: PathBuf::from(what),
        source,
    }
}

/// Compile a `--grep`/`--highlight` pattern, surfacing a bad regex with the flag
/// name (reusing [`zcolorizer::Error::BadRule`]).
fn compile_filter(flag: &str, pat: Option<&str>) -> zcolorizer::Result<Option<Regex>> {
    match pat {
        Some(p) => Regex::new(p)
            .map(Some)
            .map_err(|source| zcolorizer::Error::BadRule {
                name: flag.to_string(),
                source,
            }),
        None => Ok(None),
    }
}

/// The body of a raw line buffer (its bytes minus a trailing `\n`), decoded
/// lossily so non-UTF-8 input never crashes the colorizer.
fn line_body(raw: &[u8]) -> std::borrow::Cow<'_, str> {
    let end = if raw.last() == Some(&b'\n') {
        raw.len() - 1
    } else {
        raw.len()
    };
    String::from_utf8_lossy(&raw[..end])
}

/// Read up to `max` raw lines (newline-terminated where present) from `reader`
/// into a buffer, for `-m auto` fingerprinting + later replay.
fn sniff_reader<R: BufRead>(reader: &mut R, max: usize) -> io::Result<Vec<Vec<u8>>> {
    let mut lines = Vec::new();
    while lines.len() < max {
        let mut buf = Vec::new();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        lines.push(buf);
    }
    Ok(lines)
}

/// Sample the first `max` lines across `files` (decoded) for `-m auto`. Files are
/// re-opened for the real pass, so this only peeks. Unreadable files are skipped.
fn sniff_files(files: &[PathBuf], max: usize) -> io::Result<Vec<String>> {
    let mut lines = Vec::new();
    for path in files {
        if lines.len() >= max {
            break;
        }
        let Ok(f) = std::fs::File::open(path) else {
            continue;
        };
        let mut reader = BufReader::new(f);
        while lines.len() < max {
            let mut buf = Vec::new();
            if reader.read_until(b'\n', &mut buf)? == 0 {
                break;
            }
            lines.push(line_body(&buf).into_owned());
        }
    }
    Ok(lines)
}

/// Live-reload state for `--watch`: the config file to poll plus the CLI overrides
/// to re-apply on top of each fresh load.
struct Reloader {
    path: PathBuf,
    theme: Option<String>,
    modules: Vec<String>,
    last_mtime: Option<SystemTime>,
    last_check: Instant,
}

impl Reloader {
    fn new(path: PathBuf, theme: Option<String>, modules: Vec<String>) -> Reloader {
        let last_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        Reloader {
            path,
            theme,
            modules,
            last_mtime,
            last_check: Instant::now(),
        }
    }
}

/// Build a colorizer from a config file (or defaults if it's gone), re-applying
/// the CLI-supplied + auto-detected modules on top. Used for `--watch` reloads.
fn build_colorizer(
    path: &Path,
    theme: Option<&str>,
    modules: &[String],
) -> zcolorizer::Result<Colorizer> {
    let mut config = if path.exists() {
        Config::load(path)?
    } else {
        Config::default()
    };
    for m in modules {
        if !config.modules.iter().any(|x| x.eq_ignore_ascii_case(m)) {
            config.modules.push(m.clone());
        }
    }
    Colorizer::from_config(&config, theme)
}

/// One streaming run: holds the active colorizer and the per-line filters, and
/// (optionally) hot-reloads the colorizer when the watched config changes.
struct Session {
    colorizer: Colorizer,
    want_color: bool,
    grep: Option<Regex>,
    highlight: Option<Regex>,
    reload: Option<Reloader>,
}

impl Session {
    /// If watching, and enough time has passed, re-`stat` the config and rebuild
    /// the colorizer when its mtime changed. A failed reload keeps the previous
    /// colorizer (so a half-saved config doesn't kill the stream).
    fn maybe_reload(&mut self) {
        let Some(rl) = self.reload.as_mut() else {
            return;
        };
        if rl.last_check.elapsed() < RELOAD_THROTTLE {
            return;
        }
        rl.last_check = Instant::now();
        let mtime = std::fs::metadata(&rl.path).and_then(|m| m.modified()).ok();
        if mtime == rl.last_mtime {
            return;
        }
        rl.last_mtime = mtime;
        match build_colorizer(&rl.path, rl.theme.as_deref(), &rl.modules) {
            Ok(c) => {
                self.colorizer = c;
                eprintln!("zcolorizer: reloaded {}", rl.path.display());
            }
            Err(e) => eprintln!("zcolorizer: reload failed ({e}); keeping previous config"),
        }
    }

    /// Emit one decoded line, honoring `--grep` (drop non-matches) and
    /// `--highlight` (dim non-matches) before colorizing.
    fn emit_line<W: Write>(&self, body: &str, had_nl: bool, out: &mut W) -> io::Result<()> {
        if let Some(g) = &self.grep {
            if !g.is_match(body) {
                return Ok(());
            }
        }
        let dim = self.want_color && self.highlight.as_ref().is_some_and(|h| !h.is_match(body));
        if dim {
            // Context line: render it dim and uncolored so matches stand out.
            out.write_all(b"\x1b[2m")?;
            out.write_all(body.as_bytes())?;
            out.write_all(b"\x1b[0m")?;
        } else if self.want_color {
            out.write_all(self.colorizer.colorize_line(body).as_bytes())?;
        } else {
            out.write_all(body.as_bytes())?;
        }
        if had_nl {
            out.write_all(b"\n")?;
        }
        Ok(())
    }

    /// Replay already-buffered raw lines (the `-m auto` sniff sample) in order.
    fn replay<W: Write>(&mut self, lines: &[Vec<u8>], out: &mut W) -> io::Result<()> {
        for raw in lines {
            let had_nl = raw.last() == Some(&b'\n');
            self.emit_line(&line_body(raw), had_nl, out)?;
        }
        if !lines.is_empty() {
            out.flush()?;
        }
        Ok(())
    }

    /// Stream `reader` line-by-line to `out`. Line-buffered so `tail -f` /
    /// `journalctl -f` stay responsive.
    fn run_reader<R: BufRead, W: Write>(&mut self, mut reader: R, out: &mut W) -> io::Result<()> {
        // Fast passthrough only when nothing needs per-line work.
        if !self.want_color
            && self.grep.is_none()
            && self.highlight.is_none()
            && self.reload.is_none()
        {
            io::copy(&mut reader, out)?;
            return Ok(());
        }
        // Read raw bytes per line (not `read_line`, which errors on non-UTF-8
        // input such as binary log files).
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                break;
            }
            // Check for a config change after the line arrives (not before the
            // blocking read), so an edit applies to this very next line.
            self.maybe_reload();
            let had_nl = buf.last() == Some(&b'\n');
            self.emit_line(&line_body(&buf), had_nl, out)?;
            // Flush each line so streaming sources show up immediately.
            out.flush()?;
        }
        Ok(())
    }
}
