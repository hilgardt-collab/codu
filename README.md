# codu

**Colourful disk usage.** An interactive, navigable disk-usage browser for
modern terminals, written in Rust. Think `ncdu`, but with truecolor themes,
emoji file-type icons, a parallel scanner, and a config file that lets you
restyle every part of the interface.

```
 💽 codu  /usr                                                        🌙 Catppuccin Mocha
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
  and mouse support (click, double-click, wheel). `Esc` quits. The key
  guide at the bottom lists every shortcut for the current view, wrapping
  over as many lines as needed (`key-guide = "compact"` for one line).
- **Walk up** past where you started: the `..` row at the top of the root
  listing scans the parent directory, and above `/` it opens the volumes
  screen.
- **Volume aware**: scans stay on the starting volume by default. Other
  mounted volumes appear as mount points (flag `>`, 💾) showing that
  volume's own usage, and are not counted in the totals. `-X` crosses them.
- **Volumes screen** (`V`, `--volumes`, or `..` above `/`): every mounted and
  unmounted volume with device, filesystem, label, usage and capacity.
  Enter scans a mounted volume.
- **Themes**: eight built in (Catppuccin Mocha/Latte, Dracula, Gruvbox Dark,
  Nord, Tokyo Night, Solarized Light, and a 16-colour `ansi` fallback), a
  live in-app picker, and TOML theme files where every UI element, the bar
  glyphs and gradient, and every icon can be changed.
- **In-app theme editor**: copy any theme, edit every element's colours and
  attributes with live preview, save it to your themes directory, and set it
  as the default, all without leaving codu.
- **Group by type**: one keypress splits a listing into captioned sections
  (directories, images, video, archives, code, ...). An options popup lists
  every view toggle with its current state.
- **Emoji icons** chosen so columns never misalign, with `ascii` and `none`
  modes for terminals without emoji fonts.
- Disk usage or apparent size, binary or SI units, hard-link deduplication,
  exclude globs, single-filesystem mode, hidden-file toggle, filtering.
- Delete or move to trash with confirmation, `--read-only` to disable both.
- Rescan any directory in place.

## Install

On Arch Linux, from the AUR (`codu-git` builds the latest commit):

```sh
yay -S codu-git        # or: paru -S codu-git
```

The package also installs the [cd-on-exit](#change-directory-on-exit) shell
integration system-wide: fish picks it up automatically, bash and zsh login
shells get it from `/etc/profile.d/codu.sh`, and any other interactive shell
needs the one-liner from that section.

From source, with Rust 1.88 or newer:

```sh
cargo install --path .
```

Runs on Linux and macOS; it compiles on Windows but disk-usage sizes fall back
to apparent sizes there.

## Usage

```
codu [OPTIONS] [PATH]

  -c, --config <FILE>     alternative config file
  -T, --theme <NAME>      theme name or path to a .toml file
      --icons <MODE>      emoji | ascii | none
      --tree              start in tree view
  -x, --one-file-system   stay on the scanned volume (default)
  -X, --cross-volumes     descend into other mounted volumes
      --volumes           start on the volumes screen
      --exclude <GLOB>    exclude matching entries (repeatable)
      --apparent-size     show apparent sizes instead of disk usage
      --si                decimal units (kB, MB, GB)
  -r, --read-only         disable delete and trash
      --no-mouse          disable mouse support
  -t, --threads <N>       scanner threads (0 = CPUs)
      --list-themes       list built-in and user themes
      --dump-theme <NAME> print a theme's TOML to stdout
      --dump-config       print the default config.toml
      --shell <SHELL>     print the bash/zsh/fish function for cd-on-exit
      --cwd-file <FILE>   write the shown directory here on exit
      --no-cd             do not change directory on exit this run
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
| `y`                        | group by type (captioned sections)                    |
| `o`                        | options popup: all view toggles with their state      |
| `a`                        | disk usage ↔ apparent size                            |
| `b`                        | bar+percent → bar → percent → none                    |
| `c` `m`                    | item-count / mtime column                             |
| `e`                        | show/hide hidden entries                              |
| **Actions**                |                                                       |
| `i`                        | info popup                                            |
| `d` / `D`                  | delete permanently / move to trash (asks first)       |
| `r`                        | rescan (list: current directory; tree: selected)      |
| `T`                        | theme picker: `e` edit, `n` new copy, `S` set default |
| `V`                        | volumes screen                                        |
| `?` `F1`                   | help                                                  |
| `Esc` `Ctrl-C`             | quit (`Esc` first closes popups / clears the filter)  |

Entry flags follow ncdu: `!` unreadable, `.` unreadable subdirectory,
`<` excluded, `>` mount point of another volume, `@` symlink/special,
`H` hard link counted elsewhere, `e` empty directory.

### Volumes

Scans do not cross into other mounted volumes: a mount point inside the
scan is listed with the `>` flag and the 💾 icon, its size column shows how
much of *that volume* is in use, and it contributes nothing to the parent's
total. `i` on it shows the device, filesystem and capacity. Pass `-X` or set
`one-file-system = false` to scan across mounts instead, or flip it at
runtime with `X` in the options popup (`o`), which rescans. The status bar
shows which mode is active ("this volume" or "all volumes").

If codu ever seems to scan into other mounts, check `~/.config/codu/config.toml`
for a `one-file-system = false` line: a config file overrides the default.

The volumes screen (`V`, `codu --volumes`, or `..` from `/`) lists every
volume the system knows about:

```
 💽 codu  Volumes                                                       Catppuccin Mocha
╭──────────────────────────────────────────────────────────────────────────────────────╮
│▸   💾 /                        nvme1n1p1    btrfs      412 GiB ███░░░░░░░░░  1.8 TiB  22.6%  disk│
│    💾 /home                    nvme1n1p1    btrfs      412 GiB ███░░░░░░░░░  1.8 TiB  22.6%  disk│
│    🔌 /run/media/me/USB        sdc1         exfat      118 GiB ████████░░░░  466 GiB  25.3%  removable│
│    🌐 /mnt/nas                 nas:/export  nfs4       3.1 TiB █████████░░░  4.0 TiB  77.5%  network│
│    🔁 /dev/zram0                            swap       1.6 GiB ░░░░░░░░░░░░ 60.4 GiB   2.6%  swap│
│    💤 /dev/nvme0n1p3                        ntfs   not mounted ░░░░░░░░░░░░  1.9 TiB      -  disk│
╰────────────────────────────────────────────────────────────────────────────────── 1/6 ╯
 💽 6 volumes  ·  4 mounted  ·  ⏎ scans a mounted volume
 ↑↓ jk  move   ⏎ →  scan volume   r  refresh   Esc ← V  back   T  themes   ?  help
```

Mounted volumes come from `/proc/self/mounts` (real devices, network and
FUSE filesystems; pseudo filesystems and snap images are skipped) with usage
from `statvfs`. Unmounted volumes come from `/sys/class/block` (partitions,
and whole disks without a partition table) with filesystem type and label
from the udev database, plus active swap from `/proc/swaps`. This needs
Linux; on other platforms the screen is empty.

### Group by type

`y` splits the current listing into captioned sections so you can see at a
glance how much space each kind of file takes. Groups are ordered by their
total size (or by name when sorting by name), directories keep their own
group, and `t` still pins that group to the top. Captions are skipped by the
cursor. In the tree view the same grouping orders each directory's children
without caption rows.

```
│    📂 ..                                                                    │
│  ─ 🎬 video · 3 · 1.4 GiB ──────────────────────────────────────────────────│
│▸   🎬 talk.mkv                     774 MiB ████████████████████████  38.2%  │
│    🎬 demo.mp4                     512 MiB ████████████████░░░░░░░░  25.3%  │
│  ─ 📷 images · 12 · 380 MiB ────────────────────────────────────────────────│
│    📷 wallpaper.png                 96 MiB ███░░░░░░░░░░░░░░░░░░░░░   4.7%  │
```

### Tree view

`Tab` switches to a collapsible tree of the whole scan. Directories carry a
`+` (collapsed) or `-` (expanded) toggle; sizes, bars and percentages are all
relative to the scanned root so the picture stays consistent as you open
branches. Switching back with `Tab` drops you into the directory of whatever
you had selected.

```
 💽 codu  /home/user/Documents                                    🌙 Catppuccin Mocha
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

## Change directory on exit

With the shell integration installed, quitting codu leaves your shell in the
directory you were looking at (the listed directory in list view, the
selected entry's directory in tree view). Add one line to your shell config:

```sh
# bash: ~/.bashrc          zsh: ~/.zshrc
eval "$(codu --shell bash)"      # or: eval "$(codu --shell zsh)"
# fish: ~/.config/fish/config.fish
codu --shell fish | source
```

The function runs codu with `--cwd-file` pointing at a temporary file and
`cd`s into whatever codu wrote there. The AUR package ships it as
`/etc/profile.d/codu.sh` (bash and zsh login shells) and as an autoloaded
fish function, so only non-login bash/zsh shells need the line above. Turn the behaviour off with
`cd-on-exit = false` in the config, `W` in the options popup, or `--no-cd`
for one run. Without the integration, codu prints a one-line reminder when
you quit from somewhere other than where you started.

## Configuration

`codu --dump-config > ~/.config/codu/config.toml` writes a fully commented
default config. Every key is optional:

```toml
theme = "catppuccin-mocha"
icons = "emoji"          # emoji | ascii | none
view = "list"            # list | tree (Tab switches at runtime)
mouse = true
borders = true
key-guide = "full"       # full | compact | off
si = false
apparent-size = false
dirs-first = false
show-hidden = true
show-count = false
show-mtime = false
bar-mode = "bar-percent" # bar-percent | bar | percent | none
bar-width = 24
group-by-type = false
sort = "size"            # size | name | count | mtime
sort-reverse = false
exclude = []
one-file-system = true   # stay on the starting volume; -X to cross
threads = 0
confirm-delete = true
read-only = false
cd-on-exit = true        # with `eval "$(codu --shell bash)"` installed
date-format = "%Y-%m-%d %H:%M"
```

## Theming

### Editing themes inside codu

Press `T` for the theme picker, move to any theme and press `n` to create
a copy under a name you choose, or `e` to edit the selected theme directly.
The editor lists every element with its foreground, background, attributes
and a live sample, and everything you change is applied to the screen behind
it immediately.

```
╭ 🎨 Edit theme: My Nord (my-nord) * ───────────────────────────────────────────╮
│ element       fg           bg           attrs   preview                       │
│ name         My Nord                                                         │
│ dark         yes                                                             │
│ background   nord4        nord0        ······   Sample text 123              │
│▸dir          #ff8800      -            B·····   Sample text 123              │
│ file         nord4        -            ······   Sample text 123              │
│ ...                                                                          │
│ f  fg  g  bg  b i u d r x  bold italic underline dim reversed strike  Del ...│
│ ⏎  edit/toggle  s  save  Esc  close                                          │
│ palette: nord0 nord1 nord2 nord3 nord4 nord5 nord6 nord7 nord8 nord9 nord10 …│
╰──────────────────────────────────────────────────────────────────────────────╯
```

| Key                 | Action                                                  |
|---------------------|---------------------------------------------------------|
| `↑` `↓`             | choose an element                                       |
| `f` / `g`           | open the colour picker for the foreground / background   |
| `F` / `G`           | type a colour value directly (`none` inherits)          |
| `b` `i` `u` `d` `r` `x` | toggle bold, italic, underline, dim, reversed, strike |
| `Del`               | clear the element so it inherits from the base theme    |
| `⏎`                 | edit the natural value of the row (name, dark, colours) |
| `s`                 | save to `~/.config/codu/themes/<id>.toml`               |
| `Esc`               | close (asks once if there are unsaved changes)          |

#### Colour picker

`f`, `g`, or Enter on a colour row opens a picker with six sliders: R, G, B
and H, S, V, kept in sync, each drawn as a gradient of what that position
would give. The hex value, `rgb()` form, nearest ANSI name and nearest
256-colour index are shown, with a live sample against the element's other
colour, and the whole interface behind the modal previews the colour as you
move it.

```
╭ 🎨 Colour: dir foreground ─────────────────────────────────────────╮
│                                                                    │
│   R ██████████████┃░░░░░░░░░░░░░░░░░░░░░░░░░  93                   │
│ ▸ G ██████████████████████┃░░░░░░░░░░░░░░░░░ 141                   │
│   B ██████████████████████████████┃░░░░░░░░░ 193                   │
│                                                                    │
│   H ███████████████████████┃░░░░░░░░░░░░░░░░ 211°                  │
│   S ████████████████████┃░░░░░░░░░░░░░░░░░░░  52%                  │
│   V ██████████████████████████████┃░░░░░░░░░  76%                  │
│                                                                    │
│   value #5d8dc1   rgb(93, 141, 193)   ansi darkgray   256 #67      │
│   preview ████ Sample text 123 ████                                │
│                                                                    │
│ ↑↓ slider  ←→ ±1  H L ±10  PgUp PgDn ±16  Home End min/max  Tab … │
│ # hex  n name/palette  ⏎ apply  Esc cancel                         │
╰────────────────────────────────────────────────────────────────────╯
```

`↑↓` choose a slider, `←→` step by 1, `H`/`L` by 10, `PgUp`/`PgDn` by 16,
`Home`/`End` jump to the ends, `Tab` hops between the RGB and HSV groups.
`#` lets you type a hex value; `n` a palette key or ANSI name, which is
then kept by name in the saved file. `Enter` keeps the colour, `Esc` puts
the previous value back.

`S` in the picker writes `theme = "<id>"` to your `config.toml` so the theme
loads next time. Editing a built-in theme saves a user copy with the same id,
which shadows the built-in from then on.

### Theme files by hand

Themes are TOML files in `~/.config/codu/themes/<name>.toml` (user themes
shadow built-in ones with the same name). Start from a built-in:

```sh
mkdir -p ~/.config/codu/themes
codu --dump-theme nord > ~/.config/codu/themes/mine.toml
codu --theme mine
```

A theme has four sections. Anything you leave out is inherited from the
`ansi` base theme, so a theme can be as small as one line.

```toml
name = "Mine"
dark = true                       # picks the 🌙 / 🌞 header badge

[palette]                         # optional named colours
blue = "#89b4fa"
base = "#1e1e2e"

[styles]                          # any of the 44 element keys
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
filter tree_guide tree_toggle group`.

### About emoji widths

Terminals disagree about how many cells some emoji occupy. `codu` sidesteps
this by only shipping icons whose Unicode East Asian Width is *Wide* and by
normalising every icon to exactly two cells. If you add your own icons,
prefer emoji that have an emoji presentation by default (📁 🦀 🎵 …) and avoid
text-presentation symbols like ⚙ ⚠ 🖼, ZWJ sequences, skin-tone modifiers
and flags, which many terminals render at a different width than they report.

## Design notes

See [DESIGN.md](DESIGN.md) for the research behind the interface, the row
anatomy, the key map rationale and the architecture.

## License

Copyright © 2026 H.G.Raubenheimer (with Claude)

codu is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It comes with no warranty; see [LICENSE](LICENSE) for the full text.
