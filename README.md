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
  -C, --force-color      Color even when stdout isn't a TTY (e.g. into a pager)
      --no-color         Passthrough, never color
  -m, --module <NAME>    Enable a format module (ccze plugin port). Repeatable.
      --list-themes      List available themes (active marked with *)
      --list-modules     List available format modules
      --list-rules       List the effective rules after config merge
      --themes-json      Emit all themes as JSON
      --dump-config      Print the resolved config as TOML
```

### Format modules (ccze plugin ports)

The generic ruleset colors any log out of the box. For known formats, enable a
**module** — a port of the corresponding ccze plugin — to color structured fields
precisely:

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
