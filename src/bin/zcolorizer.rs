//! `zcolorizer` — pipe logs in, get cyberpunk-colored logs out.
//!
//! ```text
//! tail -f /var/log/syslog | zcolorizer
//! zcolorizer --theme ccze-classic access.log
//! journalctl -f | zcolorizer -t cyberpunk
//! ```

use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use zcolorizer::config::Config;
use zcolorizer::Colorizer;

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
    /// Repeatable. Merged with any `modules` in the config.
    #[arg(short = 'm', long = "module")]
    modules: Vec<String>,

    /// Config file (default: $XDG_CONFIG_HOME/zcolorizer/config.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

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
    println!();
    println!("  {M}Real-time log colorizer{N} — ccze/pygments port · 31 cyberpunk themes · 20 modules");
    println!();

    println!("{Y}  USAGE:{N} {bin} [OPTIONS] [FILES]...        {G}//{N} reads stdin when no FILES");
    println!("         tail -f /var/log/syslog | {bin} -m syslog");
    println!();

    let row = |flags: &str, desc: &str| println!("  {flags:<24}{G}//{N} {desc}");

    println!("{C}  ── INPUT ─────────────────────────────────────────────{N}");
    row("FILES...", "files to colorize (default: stdin, line-buffered)");
    println!();

    println!("{C}  ── THEME ─────────────────────────────────────────────{N}");
    row("-t, --theme NAME", "theme to use (default: neon-sprawl, alias cyberpunk)");
    row("    --list-themes", "list all themes (active marked with *)");
    row("    --themes-json", "emit every theme as JSON (for tooling)");
    println!();

    println!("{C}  ── RULES & MODULES ───────────────────────────────────{N}");
    row("-m, --module NAME", "enable a ccze format module (repeatable; `all`)");
    row("    --list-modules", "list the 20 format modules");
    row("    --list-rules", "list effective rules after config merge");
    row("-c, --config PATH", "config file (default ~/.config/zcolorizer/config.toml)");
    row("    --dump-config", "print the resolved config as TOML");
    println!();

    println!("{C}  ── OUTPUT ────────────────────────────────────────────{N}");
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

    // Merge CLI-requested modules into the config (dedup, preserve order).
    for m in &cli.modules {
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

    let unknown = config.unknown_modules();
    if !unknown.is_empty() {
        eprintln!(
            "zcolorizer: unknown module(s): {} (see --list-modules)",
            unknown.join(", ")
        );
    }

    let colorizer = Colorizer::from_config(&config, cli.theme.as_deref())?;

    let stdout = io::stdout();
    let want_color = cli.force_color || (!cli.no_color && stdout.is_terminal());
    let mut out = io::BufWriter::new(stdout.lock());

    let result = if cli.files.is_empty() {
        process_reader(BufReader::new(io::stdin().lock()), &colorizer, want_color, &mut out)
    } else {
        let mut last = Ok(());
        for path in &cli.files {
            match std::fs::File::open(path) {
                Ok(f) => {
                    if let Err(e) =
                        process_reader(BufReader::new(f), &colorizer, want_color, &mut out)
                    {
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

/// Stream `reader` line-by-line through the colorizer to `out`. Line-buffered so
/// `tail -f`/`journalctl -f` stay responsive.
fn process_reader<R: BufRead, W: Write>(
    mut reader: R,
    colorizer: &Colorizer,
    want_color: bool,
    out: &mut W,
) -> io::Result<()> {
    if !want_color {
        // Fast passthrough.
        io::copy(&mut reader, out)?;
        return Ok(());
    }
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let had_nl = line.ends_with('\n');
        let body = line.strip_suffix('\n').unwrap_or(&line);
        out.write_all(colorizer.colorize_line(body).as_bytes())?;
        if had_nl {
            out.write_all(b"\n")?;
        }
        // Flush each line so streaming sources show up immediately.
        out.flush()?;
    }
    Ok(())
}
