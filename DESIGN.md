# codu — design notes

`codu` ("colour disk usage") is an interactive, navigable disk-usage browser for
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
 │        📂 /home/sakkie/Documents/GitHub/codu/target/debug/deps          │
 │                                                                         │
 │                              q abort                                    │
 └─────────────────────────────────────────────────────────────────────────┘
```

```
 ┌ 💽 codu ─ /home/sakkie/Documents ──────────────────────── 🌙 catppuccin ┐  header
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
 │ ↑↓ jk move  PgUp PgDn page  Home End first/last  ⏎ → open  ⌫ ← up      │  key guide:
 │ Tab tree view  / filter  s sort size  n sort name  C sort items  M …    │  every key,
 │ t dirs first  y by type  … o options  T themes  ? help  Esc quit         │  wrapped
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
* in tree view, guide lines and a `+`/`-` toggle precede the icon.
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
| `Tab` / `v`              | switch list view ↔ tree view (selection is kept)    |
| `→`/`Enter`/`l` (list)   | open directory; on the root's `..`: scan the parent |
| `←`/`h`/`Backspace` (list) | go to parent directory                            |
| `+` `=` `→` `l` (tree)   | expand (`→` again steps into the first child)       |
| `-` `←` `h` (tree)       | collapse (`←` again jumps to the parent)            |
| `Space`/`Enter` (tree)   | toggle expand/collapse                              |
| `*` (tree)               | expand everything below the selection               |
| `s` `n` `C` `M`          | sort by size / name / count / mtime (again = reverse) |
| `t`                      | toggle directories-first                            |
| `y`                      | toggle group-by-type (captioned sections)           |
| `o`                      | options popup listing every toggle with its state   |
| `a`                      | toggle disk usage ↔ apparent size                   |
| `b`                      | cycle bar mode: bar+percent → bar → percent → none  |
| `c` / `m`                | toggle item-count / mtime column                    |
| `e`                      | toggle hidden (dot) entries                         |
| `/`                      | filter current listing (type, `Esc` clears)         |
| `i`                      | info popup for selected entry                       |
| `d` / `D`                | delete permanently / move to trash (confirm first)  |
| `r`                      | rescan (list: current directory; tree: selection)   |
| `T`                      | theme picker; inside: `e` edit, `n` new copy, `S` set default |
| `V`                      | volumes screen (also `..` above `/`)                |
| `?` / `F1`               | help                                                |
| `Esc` / `Ctrl-C`         | quit; `Esc` first closes popups and clears a filter |
| mouse                    | click select, double-click open/toggle, wheel scroll|

Notes: ncdu uses `g` for the graph toggle; `codu` uses `b` because `g`/`G`
are reserved for vim-style jump-to-top/bottom. `q` is deliberately unbound
so a stray keypress never exits; `Esc` is the exit key.

### The `..` row

Every listing, including the scanned root, starts with a `..` row. Inside
the scan it goes up one level. At the root it scans the parent directory
and makes it the new root, with the previous root pre-selected, so you can
walk up the filesystem without restarting. The scan is cancellable with
`Esc`, which keeps the current view.

### Volumes

* Scans stay on the starting volume by default (`one-file-system = true`).
  The check is `st_dev`, so btrfs subvolumes and bind mounts count as
  separate volumes too, which matches the mount table. `-X` crosses, and
  `X` in the options popup toggles at runtime and rescans. The status bar
  always shows the mode. `S` in the theme picker creates a *minimal*
  config.toml (only the theme line) rather than a copy of the default
  template, so today's defaults are never frozen into a user's file.
* A mount point inside the scan is a directory node with the `OTHER_FS`
  flag and zero size; the row renders that volume's own `statvfs` usage in
  the `mount` style with a `volume <dev>` label where the bar would be, so
  it is visibly not part of the scan's arithmetic.
* The volumes screen is a separate top-level state (`App::volumes`), shown
  instead of the browser. Entering a mounted volume starts a scan with it
  as the new root; the screen closes when the scan lands. Esc returns to
  the previous scan, or quits when there is none.
* Discovery (`volumes.rs`, Linux): `/proc/self/mounts` filtered to real
  devices, network and FUSE filesystems (minus portal/gvfs) and non-loop
  devices; `/proc/swaps`; `/sys/class/block` for partitions and
  partition-less disks not in the mount table, with fs type and label from
  `/run/udev/data/b<maj>:<min>` (world-readable, no root needed) and size
  from sysfs; removable/optical from sysfs; usage via `libc::statvfs`.
  RAID and LVM members are skipped (their assembled devices are listed).

### Change directory on exit

A process cannot change its parent shell's directory, so codu follows the
yazi/ranger pattern: `--cwd-file FILE` makes it write the shown directory
(list view: the listed directory; tree view: the selected entry's directory;
volumes screen: the scanned root) on exit, and `codu --shell bash|zsh|fish`
prints a wrapper function that runs the binary with a temp file and `cd`s
into the result. `cd-on-exit` (config, `W` in options, `--no-cd`) gates
both the write and the reminder that is printed when the user quits from a
different directory than they started in without the wrapper installed.

### Tree view

```
 │    📂 ..                                                                   │
 │    - 📂 /home/user/Documents               16.3 GiB ████████████████ 100.0%│
 │▸   ├─- 📂 GitHub                           11.5 GiB ███████████░░░░░  70.7%│
 │    │  ├─  🦀 main.rs                       11.5 GiB ███████████░░░░░  70.7%│
 │    │  └─  🦀 Cargo.toml                    1000 B   ░░░░░░░░░░░░░░░░   0.0%│
 │    ├─+ 📁 secret                            4.0 KiB ░░░░░░░░░░░░░░░░   0.0%│
 │  @ └─  🔗 link                                0 B   ░░░░░░░░░░░░░░░░   0.0%│
```

* The tree is the same data as the list; `expanded` is a per-node flag.
  Rows are flattened depth-first from the root on every structural change
  (expand, collapse, sort, filter, delete, rescan), not on cursor movement.
* Guide lines (`│ ├─ └─`) are drawn per ancestor level; on very deep paths
  the guides compress to `…<depth>` so the name column stays readable.
* Directories show `+`/`-`, files a blank toggle. Expanded directories use
  the open-folder icon.
* Bars and percentages are relative to the scanned root, not to siblings,
  so a branch keeps its meaning when opened next to others.
* Filtering keeps directories visible so the structure stays navigable;
  files must match.
* Switching views carries the selection: list → tree expands the path to
  the current directory; tree → list opens the selected entry's directory.

## 3. Theming

Configuration lives in `$XDG_CONFIG_HOME/codu/config.toml`; themes in
`$XDG_CONFIG_HOME/codu/themes/<name>.toml`. Built-in themes are embedded and can
be dumped with `codu --dump-theme <name>` as a starting point.

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

### Group by type

`y` buckets a directory's entries by a type label derived from the same
extension table that picks icons (images, video, archives, rust, config, …),
with directories, symlinks and special files as their own buckets and the
bare extension as a fallback label. Buckets are ordered by total size
(alphabetically under name sort), `t` pins the directory bucket first, and
each bucket gets a caption row (`─ 🎬 video · 3 · 1.4 GiB ───`). Captions are
not selectable: cursor movement, Home/End, paging and mouse clicks skip
them. The tree view applies the same ordering without captions.

### Options popup

`o` opens a popup listing every view toggle (directories first, group by
type, hidden, apparent size, count/mtime columns, bar mode, icon mode, view,
borders) with its current state; the same keys work inside it and the
listing updates behind the popup.

### Theme editor

The theme picker gains `e` (edit), `n` (new copy) and `S` (set default).
Editing works on the raw theme document (`ThemeDoc`, the same shape as the
TOML file, palette references included) rather than the resolved styles,
so saved files stay readable and keep their palette names. After every
change the document is re-resolved and applied, so the browser behind the
editor previews live. Rows cover name, dark badge, all style keys and the
six bar settings. Colour fields open a slider picker (R G B / H S V kept in
sync, hex/rgb/nearest-ANSI/256 readout, live preview; typed hex or palette
names are accepted, and palette names are stored literally so the saved
file stays readable). Text prompts start empty with the current value as a hint;
`none` clears a field, and a key whose fields are all cleared is removed so
it inherits from the `ansi` base again. Saving writes
`$XDG_CONFIG_HOME/codu/themes/<id>.toml`; `S` in the picker rewrites the
`theme =` line of `config.toml` (creating the file from the commented
default when missing).

## 4. Architecture

```
src/
  main.rs      – CLI (clap), config/theme loading, terminal setup, run loop
  cli.rs       – argument definitions
  config.rs    – config.toml model + defaults + dump
  theme.rs     – Theme model, colour/style parsing, palette resolution,
                 built-in theme registry (embedded TOML files in themes/),
                 ThemeDoc (editable form) with TOML writer and user-theme save
  theme_editor.rs – editor state machine over a ThemeDoc
  icons.rs     – icon resolution + width normalisation
  scan.rs      – parallel scanner (rayon), Node tree, hard-link dedupe,
                 progress counters, exclude globs, one-file-system
  volumes.rs   – mounted/unmounted volume discovery (Linux), statvfs usage
  app.rs       – App state: navigation stack, sort, columns, filter, popups,
                 delete/trash/refresh actions
  ui/          – rendering: browser, scanning screen, popups, formatting
  format.rs    – byte/percent/count/time formatting
```

Scanning runs on a background thread and publishes progress through atomics;
the UI polls at ~30 fps while scanning. When the scan completes the tree is
sent over a channel and the browser takes over. `r` rescans a subtree the
same way and splices it into the tree.
