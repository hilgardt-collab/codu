# cdu — design notes

`cdu` ("colour disk usage") is an interactive, navigable disk-usage browser for
modern colour terminals. It borrows the interaction model people already know
from `ncdu`/`gdu`, adds a themeable truecolor look with emoji file-type icons,
and is written in Rust on top of `ratatui` 0.30 + `crossterm` 0.29.

## 1. Research summary

### Existing tools

| Tool       | Lang | Interaction model                                    | Look                                |
|------------|------|------------------------------------------------------|-------------------------------------|
| ncdu       | Zig  | list browser, vim + arrow keys, `d` delete, `i` info | monochrome/curses, `#` bar          |
| gdu        | Go   | clone of ncdu keys, `D` trash, YAML style config     | 16-colour, block bar, only a couple of themeable elements |
| dust       | Rust | non-interactive tree with bars                       | coloured bars, no navigation        |
| dua-cli    | Rust | interactive, mark + delete, very fast parallel scan  | 16-colour                           |
| diskonaut  | Rust | treemap                                              | coloured treemap                    |

Takeaways:

* Keep the ncdu keymap. Users of ncdu/gdu should feel at home instantly.
* Scan in parallel (dua/gdu are noticeably faster than ncdu's default).
* Show progress while scanning; big trees take seconds to minutes.
* Nobody in this space does real theming. gdu themes two elements. A full
  theme file (every UI element + icon set + bar gradient) is the differentiator.
* Deleting is expected but must be guarded (confirmation, `--read-only`, and a
  safer "move to trash" alternative).

### ratatui 0.30

* `ratatui::init()` / `ratatui::restore()` handle raw mode + alternate screen.
* `Style` is `const`-constructible, `Block::title` takes `Into<Line>`,
  `Rect::layout` destructures into arrays. `crossterm` is re-exported as
  `ratatui::crossterm` (v0.29) so we never mix crossterm versions.
* Colours: `Color::Rgb`, `Color::Indexed`, named ANSI. Serde support behind
  the `serde` feature, but we parse colours ourselves for friendlier errors and
  palette references.

### Emoji in terminals

`unicode-width` (UAX #11) is what ratatui uses to place graphemes in cells.
It returns 2 for code points with East Asian Width = `W`, and 1 for `N`/`A`.
Terminals, however, render *any* code point with `Emoji_Presentation=Yes` as
two cells, and many render `N` + VS16 (U+FE0F) as two cells as well. ZWJ
sequences and flags are inconsistent across terminals.

Design rules that follow from this:

1. **Default icons are only emoji whose East Asian Width is `W`**
   (e.g. 📁 📂 📄 🔗 📷 🎵 🎬 📦 📜 🦀 🐍 📝 🔧 🔒). Icons like 🖼 ⚙ ⚠ 🗑 🗂 ⏱
   are `N` and are avoided by default — they misalign columns.
2. **The icon column has a fixed width of 2 cells.** Every icon is measured
   with `unicode-width`, padded with a space if it measures 1, and rejected
   (replaced by the fallback) if it measures more than 2. Misaligned rows are
   the single most visible failure mode, so this is enforced centrally.
3. **Icons are fully user-configurable** and can be switched to an ASCII
   set or turned off (`icons = "emoji" | "ascii" | "none"`), so terminals
   without an emoji font still get a clean interface.
4. Never use ZWJ sequences, skin-tone modifiers, or flags in built-in themes.

## 2. Interface design

### Screens

```
 ┌─ scanning ──────────────────────────────────────────────────────────────┐
 │                                                                         │
 │        ⏳  Scanning /home/sakkie/Documents                              │
 │                                                                         │
 │        📄 84,233 items   💾 21.4 GiB   ⌛ 2.3s                          │
 │        📂 /home/sakkie/Documents/GitHub/cdu/target/debug/deps           │
 │                                                                         │
 │                              q abort                                    │
 └─────────────────────────────────────────────────────────────────────────┘
```

```
 ┌ 💽 cdu ─ /home/sakkie/Documents ───────────────────────── 🌙 catppuccin ┐  header
 │    📂 ..                                                                │
 │  ▸ 📁 GitHub        12.4 GiB ████████████████░░░░░░░░ 61.2%   45,120   │  selected
 │    📦 archive.tar.gz 3.1 GiB ██████░░░░░░░░░░░░░░░░░░ 15.3%            │
 │    📷 wallpaper.png  1.2 GiB ██░░░░░░░░░░░░░░░░░░░░░░  5.9%            │
 │  H 🎬 talk.mkv       812 MiB █░░░░░░░░░░░░░░░░░░░░░░░  3.9%            │  flag H = hardlink
 │  ! 📁 secret          ---    ░░░░░░░░░░░░░░░░░░░░░░░░  0.0%            │  flag ! = unreadable
 │    👻 .cache          32 MiB ░░░░░░░░░░░░░░░░░░░░░░░░  0.2%    1,203   │  hidden entry
 │                                                                         │
 ├─────────────────────────────────────────────────────────────────────────┤
 │ 💾 20.3 GiB in 45,231 items  ·  🔽 size  ·  disk usage                  │  status
 │ ↑↓ move  ⏎ open  ⌫ up  s size  n name  g graph  i info  d del  ? help  │  key bar
 └─────────────────────────────────────────────────────────────────────────┘
```

Popups (centered, over a dimmed browser): **help**, **info**, **confirm
delete/trash**, **theme picker** (live preview while moving the cursor),
**error**.

### Row anatomy

```
 [mark][flag] [icon] name………………  size   [bar]  [percent]  [count]  [mtime]
```

* `mark` – `▸` on the selected row (theme-configurable).
* `flag` – one char, ncdu-compatible: `!` read error, `.` error in a
  subdirectory, `@` symlink/special, `H` hard link already counted,
  `e` empty dir, `>` other filesystem, `<` excluded.
* `icon` – 2-cell emoji/ASCII column from the theme's icon map
  (dir / open dir / file / symlink / hidden / by extension).
* `name` – truncated with `…` to fit; hidden entries use the `hidden` style.
* `size` – right-aligned, unit styled separately (`12.4 GiB`), binary by
  default, `--si` for decimal.
* `bar` – fixed width (default 24), relative to the largest entry in the
  listing (like ncdu). Filled/empty glyphs and colours are themeable; with
  `bar.gradient = true` the fill colour interpolates low→mid→high by the
  entry's share of the parent.
* `percent` – share of the current directory.
* `count`, `mtime` – optional columns, toggled with `c` and `m`.

Columns collapse gracefully: below ~70 cells the bar shrinks, below ~50 the
percent/count columns drop, so the tool stays usable in a split pane.

### Key map (ncdu-compatible, plus extras)

| Keys                     | Action                                              |
|--------------------------|-----------------------------------------------------|
| `↑`/`k`, `↓`/`j`         | move selection                                      |
| `PgUp`/`PgDn`, `Home`/`End`, `g`/`G` (vim) | page / jump                       |
| `→`/`Enter`/`l`          | open directory                                      |
| `←`/`h`/`Backspace`      | go to parent                                        |
| `s` `n` `C` `M`          | sort by size / name / count / mtime (again = reverse) |
| `t`                      | toggle directories-first                            |
| `a`                      | toggle disk usage ↔ apparent size                   |
| `b`                      | cycle bar mode: bar+percent → bar → percent → none  |
| `c` / `m`                | toggle item-count / mtime column                    |
| `e`                      | toggle hidden (dot) entries                         |
| `/`                      | filter current listing (type, `Esc` clears)         |
| `i`                      | info popup for selected entry                       |
| `d` / `D`                | delete permanently / move to trash (confirm first)  |
| `r`                      | rescan the current directory                        |
| `T`                      | theme picker                                        |
| `?`                      | help                                                |
| `q` / `Ctrl-C`           | quit (`Esc` closes popups)                          |
| mouse                    | click select, double-click open, wheel scroll       |

Note: ncdu uses `g` for the graph toggle; `cdu` uses `b` because `g`/`G`
are reserved for vim-style jump-to-top/bottom.

## 3. Theming

Configuration lives in `$XDG_CONFIG_HOME/cdu/config.toml`; themes in
`$XDG_CONFIG_HOME/cdu/themes/<name>.toml`. Built-in themes are embedded and can
be dumped with `cdu --dump-theme <name>` as a starting point.

Built-in: `catppuccin-mocha` (default), `catppuccin-latte`, `dracula`,
`gruvbox-dark`, `nord`, `tokyo-night`, `solarized-light`, `ansi`
(16-colour fallback that inherits the terminal palette).

Theme file shape:

```toml
name = "Catppuccin Mocha"

[palette]                 # optional named colours, referenced by name below
base = "#1e1e2e"
text = "#cdd6f4"
blue = "#89b4fa"

[styles]                  # every UI element; each is { fg, bg, bold, italic, underline, dim }
background = { bg = "base" }
header     = { fg = "text", bg = "surface0", bold = true }
selected   = { fg = "base", bg = "blue", bold = true }
dir        = { fg = "blue", bold = true }
file       = { fg = "text" }
hidden     = { fg = "overlay1", italic = true }
size       = { fg = "green" }
bar_filled = { fg = "blue" }
bar_empty  = { fg = "surface1" }
# ...see themes/catppuccin-mocha.toml for the full list

[bar]
filled   = "█"
empty    = "░"
gradient = true
low  = "green"
mid  = "yellow"
high = "red"

[icons]                   # all optional; unset keys fall back to the built-in set
dir = "📁"
dir_open = "📂"
file = "📄"
symlink = "🔗"
hidden = "👻"
parent = "📂"
[icons.ext]
rs = "🦀"
png = "📷"
```

Colours accept `#rrggbb`, `rgb(r,g,b)`, an ANSI index `0`–`255`, a named
ANSI colour (`red`, `lightblue`, `darkgray`, …), `reset`, or a `[palette]`
key. Unknown keys produce a warning, not a crash. Any theme key that is
missing falls back to the `ansi` theme so partial themes work.

## 4. Architecture

```
src/
  main.rs      – CLI (clap), config/theme loading, terminal setup, run loop
  cli.rs       – argument definitions
  config.rs    – config.toml model + defaults + dump
  theme/       – Theme model, colour/style parsing, palette resolution,
                 built-in theme registry (embedded TOML files in themes/)
  icons.rs     – icon resolution + width normalisation
  scan.rs      – parallel scanner (rayon), Node tree, hard-link dedupe,
                 progress counters, exclude globs, one-file-system
  app.rs       – App state: navigation stack, sort, columns, filter, popups,
                 delete/trash/refresh actions
  ui/          – rendering: browser, scanning screen, popups, formatting
  format.rs    – byte/percent/count/time formatting
```

Scanning runs on a background thread and publishes progress through atomics;
the UI polls at ~30 fps while scanning. When the scan completes the tree is
sent over a channel and the browser takes over. `r` rescans a subtree the
same way and splices it into the tree.
