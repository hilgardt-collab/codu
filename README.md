# cdu

**Colourful disk usage.** An interactive, navigable disk-usage browser for
modern terminals, written in Rust. Think `ncdu`, but with truecolor themes,
emoji file-type icons, a parallel scanner, and a config file that lets you
restyle every part of the interface.

```
 💽 cdu  /usr                                                         🌙 Catppuccin Mocha
╭────────────────────────────────────────────────────────────────────────────────────────╮
│▸   📁 share                                    17.1 GiB ████████████████████████  45.9%│
│    📚 lib                                      16.2 GiB ███████████████████████░  43.6%│
│    🧩 bin                                       2.2 GiB ███░░░░░░░░░░░░░░░░░░░░░   5.9%│
│    📁 lib32                                     1.0 GiB █░░░░░░░░░░░░░░░░░░░░░░░   2.8%│
│    📁 include                                   544 MiB █░░░░░░░░░░░░░░░░░░░░░░░   1.4%│
│    🧬 src                                       132 MiB ░░░░░░░░░░░░░░░░░░░░░░░░   0.3%│
│  @ 🔗 lib64                                     4.0 KiB ░░░░░░░░░░░░░░░░░░░░░░░░   0.0%│
╰────────────────────────────────────────────────────────────────────────────────── 1/10 ╯
 💾 37.2 GiB in 797,837 items  ·  ↓ size  ·  disk usage
 ↑↓  move   ⏎  open   ⌫  up   s  size   n  name   b  bar   a  apparent   /  filter   i  info
```

## Features

- **Fast parallel scan** using a work-stealing thread pool, with a live
  progress screen (items, bytes, elapsed, current path).
- **Two views**: an ncdu-style directory list, and a collapsible tree of the
  whole scan with `+`/`-` toggles. `Tab` switches between them and keeps
  your selection.
- **ncdu-compatible keys** so you already know how to use it, plus vim keys
  and mouse support (click, double-click, wheel). `Esc` quits.
- **Walk up** past where you started: the `..` row at the top of the root
  listing scans the parent directory.
- **Themes**: eight built in (Catppuccin Mocha/Latte, Dracula, Gruvbox Dark,
  Nord, Tokyo Night, Solarized Light, and a 16-colour `ansi` fallback), a
  live in-app picker, and TOML theme files where every UI element, the bar
  glyphs and gradient, and every icon can be changed.
- **Emoji icons** chosen so columns never misalign, with `ascii` and `none`
  modes for terminals without emoji fonts.
- Disk usage or apparent size, binary or SI units, hard-link deduplication,
  exclude globs, single-filesystem mode, hidden-file toggle, filtering.
- Delete or move to trash with confirmation, `--read-only` to disable both.
- Rescan any directory in place.

## Install

```sh
cargo install --path .
```

Requires Rust 1.88 or newer. Runs on Linux and macOS; it compiles on Windows
but disk-usage sizes fall back to apparent sizes there.

## Usage

```
cdu [OPTIONS] [PATH]

  -c, --config <FILE>     alternative config file
  -T, --theme <NAME>      theme name or path to a .toml file
      --icons <MODE>      emoji | ascii | none
      --tree              start in tree view
  -x, --one-file-system   do not cross filesystem boundaries
      --exclude <GLOB>    exclude matching entries (repeatable)
      --apparent-size     show apparent sizes instead of disk usage
      --si                decimal units (kB, MB, GB)
  -r, --read-only         disable delete and trash
      --no-mouse          disable mouse support
  -t, --threads <N>       scanner threads (0 = CPUs)
      --list-themes       list built-in and user themes
      --dump-theme <NAME> print a theme's TOML to stdout
      --dump-config       print the default config.toml
```

### Keys

| Keys                       | Action                                                |
|----------------------------|-------------------------------------------------------|
| `↑` `k` / `↓` `j`          | move selection                                        |
| `PgUp` `PgDn`, `Home` `g`, `End` `G` | page, first, last                           |
| `Tab` `v`                  | switch between list view and tree view                |
| `/`                        | filter the listing (`Esc` clears)                     |
| **List view**              |                                                       |
| `→` `⏎` `l`                | open directory; on the `..` row at the top of the scanned root, scan the parent directory |
| `←` `⌫` `h`                | parent directory                                      |
| **Tree view**              |                                                       |
| `+` `=` `→` `l`            | expand (`→` again steps into the first child)         |
| `-` `←` `h`                | collapse (`←` again jumps to the parent)              |
| `Space` `⏎`                | toggle                                                |
| `*`                        | expand everything below the selection                 |
| `⌫`                        | jump to the parent row                                |
| **Sort & view**            |                                                       |
| `s` `n` `C` `M`            | sort by size / name / items / mtime (again: reverse)  |
| `t`                        | directories first                                     |
| `a`                        | disk usage ↔ apparent size                            |
| `b`                        | bar+percent → bar → percent → none                    |
| `c` `m`                    | item-count / mtime column                             |
| `e`                        | show/hide hidden entries                              |
| **Actions**                |                                                       |
| `i`                        | info popup                                            |
| `d` / `D`                  | delete permanently / move to trash (asks first)       |
| `r`                        | rescan (list: current directory; tree: selected)      |
| `T`                        | theme picker with live preview                        |
| `?` `F1`                   | help                                                  |
| `Esc` `Ctrl-C`             | quit (`Esc` first closes popups / clears the filter)  |

Entry flags follow ncdu: `!` unreadable, `.` unreadable subdirectory,
`<` excluded, `>` other filesystem, `@` symlink/special, `H` hard link
counted elsewhere, `e` empty directory.

### Tree view

`Tab` switches to a collapsible tree of the whole scan. Directories carry a
`+` (collapsed) or `-` (expanded) toggle; sizes, bars and percentages are all
relative to the scanned root so the picture stays consistent as you open
branches. Switching back with `Tab` drops you into the directory of whatever
you had selected.

```
 💽 cdu  /home/user/Documents                                     🌙 Catppuccin Mocha
╭────────────────────────────────────────────────────────────────────────────────────╮
│    📂 ..                                                                           │
│    - 📂 /home/user/Documents               16.3 GiB ████████████████████████ 100.0%│
│▸   ├─- 📂 GitHub                           11.5 GiB █████████████████░░░░░░░  70.7%│
│    │  ├─  🦀 main.rs                       11.5 GiB █████████████████░░░░░░░  70.7%│
│    │  └─  🦀 Cargo.toml                    1000 B   ░░░░░░░░░░░░░░░░░░░░░░░░   0.0%│
│    ├─  📦 archive.tar.gz                    2.9 GiB ████░░░░░░░░░░░░░░░░░░░░  17.7%│
│    ├─+ 📁 secret                            4.0 KiB ░░░░░░░░░░░░░░░░░░░░░░░░   0.0%│
│  @ └─  🔗 link                                0 B   ░░░░░░░░░░░░░░░░░░░░░░░░   0.0%│
╰────────────────────────────────────────────────────────────────────────────── 3/11 ╯
 💾 16.3 GiB in 9 items  ·  tree  ·  ↓ size  ·  disk usage
 ↑↓  move   + -  expand/collapse   *  expand all   Tab  list   s  size   n  name
```

## Configuration

`cdu --dump-config > ~/.config/cdu/config.toml` writes a fully commented
default config. Every key is optional:

```toml
theme = "catppuccin-mocha"
icons = "emoji"          # emoji | ascii | none
view = "list"            # list | tree (Tab switches at runtime)
mouse = true
borders = true
si = false
apparent-size = false
dirs-first = false
show-hidden = true
show-count = false
show-mtime = false
bar-mode = "bar-percent" # bar-percent | bar | percent | none
bar-width = 24
sort = "size"            # size | name | count | mtime
sort-reverse = false
exclude = []
one-file-system = false
threads = 0
confirm-delete = true
read-only = false
date-format = "%Y-%m-%d %H:%M"
```

## Theming

Themes are TOML files in `~/.config/cdu/themes/<name>.toml` (user themes
shadow built-in ones with the same name). Start from a built-in:

```sh
mkdir -p ~/.config/cdu/themes
cdu --dump-theme nord > ~/.config/cdu/themes/mine.toml
cdu --theme mine
```

A theme has four sections. Anything you leave out is inherited from the
`ansi` base theme, so a theme can be as small as one line.

```toml
name = "Mine"
dark = true                       # picks the 🌙 / 🌞 header badge

[palette]                         # optional named colours
blue = "#89b4fa"
base = "#1e1e2e"

[styles]                          # any of the 43 element keys
background = { bg = "base" }
dir        = { fg = "blue", bold = true }
selected   = { bg = "#45475a", bold = true }
size       = { fg = "green" }     # ANSI names work too

[bar]
filled   = "█"
empty    = "░"
gradient = true                   # colour the bar by share of the directory
low  = "green"
mid  = "yellow"
high = "red"

[icons]                           # override any icon; must render as 2 cells
dir = "📁"
hidden = "👻"
[icons.ext]
rs = "🦀"
[icons.names]
"node_modules" = "📦"
```

Colours accept `#rgb`, `#rrggbb`, `rgb(r,g,b)`, an ANSI index `0`–`255`,
ANSI names (`red`, `lightblue`, `darkgray`, …), `reset`, or a `[palette]` key.
Style tables accept `fg`, `bg`, `bold`, `dim`, `italic`, `underline`,
`reversed`, `crossed_out`. Problems in a theme are reported in the status bar
rather than aborting.

Style keys: `background header header_title header_path header_theme border
border_title selected marker dir file symlink special hidden excluded error
flag size size_unit percent count mtime bar_filled bar_empty status
status_accent keybar_key keybar_label popup popup_border popup_title help_key
help_desc danger warning success spinner scan_label scan_value scan_path
filter tree_guide tree_toggle`.

### About emoji widths

Terminals disagree about how many cells some emoji occupy. `cdu` sidesteps
this by only shipping icons whose Unicode East Asian Width is *Wide* and by
normalising every icon to exactly two cells. If you add your own icons,
prefer emoji that have an emoji presentation by default (📁 🦀 🎵 …) and avoid
text-presentation symbols like ⚙ ⚠ 🖼, ZWJ sequences, skin-tone modifiers
and flags, which many terminals render at a different width than they report.

## Design notes

See [DESIGN.md](DESIGN.md) for the research behind the interface, the row
anatomy, the key map rationale and the architecture.

## License

MIT
