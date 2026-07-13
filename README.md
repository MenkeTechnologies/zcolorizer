```
███████╗ ██████╗ ██████╗ ██╗      ██████╗ ██████╗ ██╗███████╗███████╗██████╗
╚══███╔╝██╔════╝██╔═══██╗██║     ██╔═══██╗██╔══██╗██║╚══███╔╝██╔════╝██╔══██╗
  ███╔╝ ██║     ██║   ██║██║     ██║   ██║██████╔╝██║  ███╔╝ █████╗  ██████╔╝
 ███╔╝  ██║     ██║   ██║██║     ██║   ██║██╔══██╗██║ ███╔╝  ██╔══╝  ██╔══██╗
███████╗╚██████╗╚██████╔╝███████╗╚██████╔╝██║  ██║██║███████╗███████╗██║  ██║
╚══════╝ ╚═════╝ ╚═════╝ ╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝╚══════╝╚══════╝╚═╝  ╚═╝
```

# zcolorizer

> Real-time log file colorizer — a Rust port of [ccze] + the pygments regex→token
> idea, with **fully customizable rules** and **swappable themes**. Cyberpunk by default.

```
tail -f /var/log/syslog | zcolorizer
journalctl -f          | zcolorizer --theme cyberpunk
zcolorizer access.log  | less -R
```

## Why it's built this way

Three decoupled pieces, so you can change one without disturbing the others:

| Piece | Decides | Lives in |
|---|---|---|
| **Rules** (`src/rules.rs`) | *which* text gets a semantic token (`date`, `error`, `host`, `ip`, …) | regexes — fully editable in the config |
| **Themes** (`src/theme.rs`) | *what each token looks like* (color + bold/underline/…) | a named token→style map you can swap live |
| **Engine** (`src/engine.rs`) | runs rules over each line, paints claimed spans with the active theme | — |

A **rule** is a regex. Its **named capture groups *are* tokens** — so
`(?P<date>\d{4}-\d\d-\d\d)\s+(?P<host>\S+)` paints the date and host fields in
their respective theme colors from one pattern. Rules with no named groups tag
their whole match with a single `token`. This is exactly the pygments
"regex → token" model, and it's what makes the rules fully yours: edit them in
TOML, no recompile.

Because color lives entirely in the **theme**, switching themes recolors every
log without touching a single rule — that's the **"pick the theme from zgui"**
flow. `zcolorizer --themes-json` emits every theme as JSON for the picker to
render swatches from.

## Build

```sh
cargo build --release
./target/release/zcolorizer --help
```

## Usage

```
zcolorizer [OPTIONS] [FILES]...

  -t, --theme <NAME>     Theme to use (overrides config). See --list-themes.
  -c, --config <PATH>    Config file (default: ~/.config/zcolorizer/config.toml)
  -w, --watch            Reload config/theme live when the file changes
  -C, --force-color      Color even when stdout isn't a TTY (e.g. into a pager)
      --no-color         Passthrough, never color
  -m, --module <NAME>    Enable a format module (ccze plugin port). Repeatable.
                         Use `auto` to sniff the input and enable what's present.
  -g, --grep <REGEX>     Show only matching lines (still colorized)
  -H, --highlight <REGEX> Spotlight: dim the lines that don't match
      --novelty          Intensity tracks statistical novelty, not pattern class
      --novelty-decay <D> Age the novelty counts (0<D<=1); implies --novelty
      --list-themes      List available themes (active marked with *)
      --list-modules     List available format modules
      --list-rules       List the effective rules after config merge
      --themes-json      Emit all themes as JSON
      --dump-config      Print the resolved config as TOML
```

### Auto-detection, filtering & live reload

```sh
journalctl -f          | zcolorizer -m auto        # sniff & enable matching modules
tail -f app.log        | zcolorizer -g '(?i)error' # only error lines, still colored
tail -f app.log        | zcolorizer -H ERROR       # spotlight errors, dim the rest
tail -f /var/log/syslog | zcolorizer --watch       # recolors live as you edit the config
```

* **`-m auto`** samples the first lines, fingerprints the format from each
  module's start-anchored signature, and enables only the formats actually
  present. Formats identifiable only mid-line (JSON, logfmt) aren't auto-enabled
  — name them with `-m json`. No input lines are dropped: the sniffed sample is
  replayed.
* **`-g/--grep`** filters to matching lines while keeping color (unlike `| grep`,
  which strips it). **`-H/--highlight`** keeps every line but dims the
  non-matches so matches stand out in context.
* **`-w/--watch`** re-reads the config file when its mtime changes mid-stream,
  rebuilding rules + theme without restarting — the live half of the
  "pick the theme from zgui" flow. A half-saved (invalid) config is ignored,
  keeping the previous one until the next good save.

### Novelty coloring

```sh
tail -f /var/log/syslog | zcolorizer --novelty            # repetitive lines self-dim
tail -f app.log         | zcolorizer --novelty-decay 0.9  # also forget stale patterns
```

`--novelty` makes colour *intensity* a function of statistical novelty instead of
pattern class. Each line's variable-value spans — numbers, IPs, PIDs, sizes,
versions, dates, times, addresses — are masked to a **template**; the template is
hashed and scored against an online frequency model. First-seen and rare
templates render **bright + bold** (and reverse on first sight); a template that
dominates a repetitive stream **dims**. No rule and no prior knowledge of the log
format is needed — a `tail -f` of chatter self-quiets while an anomalous line
lights up. The colours themselves stay theme-driven; only the intensity moves.

`--novelty-decay D` (with `0 < D <= 1`, and implying `--novelty`) ages the counts
by factor `D` periodically, so a pattern that stops recurring is forgotten and
re-lights as novel if it returns. Omit it for pure cumulative frequency.

### Format modules

The generic ruleset colors any log out of the box. For known formats, enable a
**module** to color structured fields precisely. Modules come in two families: the
classic **ccze plugin ports** (syslog, httpd, squid, postfix, exim, …) and the
**modern formats** ccze never covered (systemd/journald, nginx, JSON, logfmt,
Postgres/MySQL/Redis/Mongo, Docker/Kubernetes, app frameworks, …):

```sh
tail -f /var/log/syslog          | zcolorizer -m syslog
zcolorizer -m httpd access.log   | less -R
zcolorizer -m all mixed.log      # enable every module; each only fires on matching lines
```

Modules layer on top of the generic rules (they win on overlap, but a free-text
message still flows through the generic word-colorizer — exactly ccze's
module-then-wordcolor design). Run `zcolorizer --list-modules` for the full list.

With no `FILES`, reads stdin (line-buffered, so `tail -f`/`journalctl -f` stream
live). Color auto-disables when piped to a non-terminal unless `--force-color`.

## Configuration

Copy [`config.example.toml`](config.example.toml) to
`~/.config/zcolorizer/config.toml`. Everything is optional — a one-liner is valid:

```toml
theme = "cyberpunk"
```

Override just a few colors of a builtin theme:

```toml
[[themes]]
name = "cyberpunk"
[themes.styles.error]
fg = "#ff003c"
bold = true
underline = true
```

Add your own rules (they win over builtins by default — `rules_mode = "prepend"`):

```toml
[[rules]]
name = "uuid"
pattern = '\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b'
token = "address"
ignore_case = true
```

Color forms: `"#ff00aa"` (truecolor), `"bright_cyan"` (named ANSI),
`{ index = 213 }` (256-color). Style flags: `bold dim italic underline blink reverse`.

## Themes

**31 cyberpunk palettes**, ported from the shared MenkeTechnologies theme set
(`iftoprs`/`storageshower`): `neon-sprawl` (the default, a.k.a. `cyberpunk`),
`acid-rain`, `synth-wave`, `blade-runner`, `night-city`, `toxic-waste`,
`megacorp`, `zaibatsu`, … plus `ccze-classic` for 16-color parity. Run
`zcolorizer --list-themes`, pick with `-t/--theme`, customize or add your own in
the config.

## Status

Working: streaming CLI, regex→token engine with span-ownership precedence, TOML
config (custom rules + theme/color overrides), truecolor/256/named color, all 31
ported themes, the ccze format modules, and `--themes-json` for tooling.

# created by MenkeTechnologies

[ccze]: https://github.com/cornet/ccze
