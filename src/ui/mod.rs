//! Rendering. Each frame is drawn from scratch from the [`App`] state.

mod browser;
mod popups;
mod scanning;
mod volumes;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame
        .buffer_mut()
        .set_style(area, app.theme.styles.background);

    let drew_screen = if app.volumes.is_some() {
        volumes::draw(frame, app);
        true
    } else if app.tree.is_some() {
        browser::draw(frame, app);
        true
    } else {
        false
    };
    if app.is_scanning() {
        if drew_screen {
            dim(frame.buffer_mut(), area);
        }
        scanning::draw(frame, app);
        return;
    }
    popups::draw(frame, app);
}

/// A rect of at most `width`×`height` centred in `area`.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// The app name shown at the left of every screen's header.
fn header_title(app: &App) -> String {
    if app.icons.emoji() {
        format!(" {} codu ", app.icons.app)
    } else {
        " codu ".to_string()
    }
}

/// The theme name shown at the right of every screen's header.
fn theme_label(app: &App) -> String {
    let mood = match (app.icons.emoji(), app.theme.dark) {
        (false, _) => "",
        (true, Some(true)) => "🌙 ",
        (true, Some(false)) => "🌞 ",
        (true, None) => "🎨 ",
    };
    format!(" {mood}{} ", app.theme.name)
}

/// Dim everything currently drawn in `area` (used underneath popups).
pub fn dim(buf: &mut Buffer, area: Rect) {
    buf.set_style(area, Style::new().add_modifier(Modifier::DIM));
}

#[cfg(test)]
pub mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::config::Config;
    use crate::icons::{IconMode, IconSet};
    use crate::scan::{Kind, Node, ScanOptions, flags};
    use crate::theme::Theme;

    fn leaf(name: &str, size: u64, kind: Kind, fl: u8) -> Node {
        Node {
            name: name.into(),
            kind,
            size,
            apparent: size,
            mtime: 1_700_000_000,
            items: 0,
            flags: fl,
            expanded: false,
            children: vec![],
        }
    }

    fn dir(name: &str, children: Vec<Node>) -> Node {
        let size = children.iter().map(|c| c.size).sum::<u64>() + 4096;
        let items = children.iter().map(|c| 1 + c.items).sum();
        Node {
            name: name.into(),
            kind: Kind::Dir,
            size,
            apparent: size,
            mtime: 1_700_000_000,
            items,
            flags: 0,
            expanded: false,
            children,
        }
    }

    pub fn sample_tree() -> Node {
        dir(
            "/home/user/Documents",
            vec![
                dir(
                    "GitHub",
                    vec![
                        leaf("main.rs", 12_400_000_000, Kind::File, 0),
                        leaf("Cargo.toml", 1000, Kind::File, 0),
                    ],
                ),
                leaf("archive.tar.gz", 3_100_000_000, Kind::File, 0),
                leaf("wallpaper.png", 1_200_000_000, Kind::File, 0),
                leaf("talk.mkv", 812_000_000, Kind::File, flags::HARDLINK),
                dir("secret", vec![]),
                leaf(".cache", 32_000_000, Kind::File, 0),
                leaf("link", 0, Kind::Symlink, 0),
            ],
        )
    }

    pub fn app_for(tree: Node, theme: &str, icons: IconMode) -> App {
        let config = Config {
            icons,
            ..Config::default()
        };
        let theme = Theme::load(theme).unwrap();
        let icon_set = IconSet::build(icons, &theme.icons);
        let mut app = App::new(
            config,
            theme,
            icon_set,
            PathBuf::from("/home/user/Documents"),
            ScanOptions::default(),
            false,
        );
        // Discard the real background scan and install the fixture.
        app.scan = None;
        app.tree = Some(tree);
        app.tree.as_mut().unwrap().expanded = true;
        app.rebuild_rows(None);
        app.cursor = 1;
        let _ = Arc::strong_count(&app.scan_options);
        app
    }

    pub fn render(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buf.area.height {
            let mut line = String::new();
            let mut x = 0;
            while x < buf.area.width {
                let sym = buf[(x, y)].symbol();
                line.push_str(sym);
                x += (crate::format::width(sym) as u16).max(1);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    #[test]
    fn browser_renders_expected_layout() {
        let mut tree = sample_tree();
        tree.children[4].flags |= flags::ERR;
        let mut app = app_for(tree, "catppuccin-mocha", IconMode::Emoji);
        app.show_count = true;
        let screen = render(&mut app, 100, 16);
        println!("{screen}");
        assert!(screen.contains("💽 codu"));
        assert!(screen.contains("/home/user/Documents"));
        assert!(screen.contains("Catppuccin Mocha"));
        assert!(screen.contains("▸ "));
        assert!(screen.contains("📁 GitHub"));
        assert!(screen.contains("📦 archive.tar.gz"));
        assert!(screen.contains("H 🎬 talk.mkv"));
        assert!(screen.contains("! ❗ secret"));
        assert!(screen.contains("👻 .cache"));
        assert!(screen.contains("@ 🔗 link"));
        assert!(screen.contains("11.5 GiB"));
        assert!(screen.contains("📂 .."));
        assert!(screen.contains("this volume"));
        assert!(screen.contains("2/8"));
        assert!(screen.contains("↑↓ jk  move"));
        // The full guide wraps: every shortcut is visible even at 100 columns.
        for key in [
            "Esc  quit",
            "D  trash",
            "y  by type",
            "?  help",
            "Tab  tree view",
        ] {
            assert!(screen.contains(key), "missing {key}:\n{screen}");
        }
        assert!(!screen.contains("expand all"));
    }

    #[test]
    fn rows_stay_aligned_with_emoji_icons() {
        let mut app = app_for(sample_tree(), "nord", IconMode::Emoji);
        let screen = render(&mut app, 90, 20);
        // Every entry row must have its size unit in the same column.
        let cols: Vec<usize> = screen
            .lines()
            .filter(|l| l.starts_with('│'))
            .filter_map(|l| {
                let idx = l
                    .find(" GiB")
                    .or_else(|| l.find(" MiB"))
                    .or_else(|| l.find(" B "))?;
                Some(crate::format::width(&l[..idx]))
            })
            .collect();
        assert!(cols.len() >= 5, "{screen}");
        assert!(
            cols.iter().all(|&c| c == cols[0]),
            "misaligned columns:\n{screen}"
        );
    }

    #[test]
    fn narrow_terminal_drops_optional_columns() {
        let mut app = app_for(sample_tree(), "ansi", IconMode::Ascii);
        app.show_count = true;
        app.show_mtime = true;
        let screen = render(&mut app, 44, 26);
        println!("{screen}");
        assert!(screen.contains("GitHub"));
        assert!(
            !screen.contains("2023-"),
            "mtime should be dropped at 44 cols"
        );
        assert!(screen.lines().all(|l| crate::format::width(l) <= 44));
    }

    #[test]
    fn tree_view_renders_guides_and_toggles() {
        let mut app = app_for(sample_tree(), "catppuccin-mocha", IconMode::Emoji);
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Tab,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('+'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        let screen = render(&mut app, 100, 22);
        println!("{screen}");
        assert!(screen.contains("- 📂 /home/user/Documents"));
        assert!(screen.contains("├─- 📂 GitHub"));
        assert!(screen.contains("│  ├─  🦀 main.rs"));
        assert!(screen.contains("│  └─  🦀 Cargo.toml"));
        assert!(screen.contains("├─  📦 archive.tar.gz"));
        assert!(screen.contains("├─+ 📁 secret") || screen.contains("├─+ ❗ secret"));
        assert!(screen.contains("└─  🔗 link"));
        assert!(screen.contains("tree"));
        assert!(screen.contains("+ →  expand") && screen.contains("- ←  collapse"));
        // Size column stays aligned regardless of depth.
        let cols: Vec<usize> = screen
            .lines()
            .filter(|l| l.starts_with('│'))
            .filter_map(|l| {
                let idx = l
                    .find(" GiB")
                    .or_else(|| l.find(" MiB"))
                    .or_else(|| l.find(" KiB"))
                    .or_else(|| l.find(" B "))?;
                Some(crate::format::width(&l[..idx]))
            })
            .collect();
        assert!(cols.len() >= 6, "{screen}");
        assert!(cols.iter().all(|&c| c == cols[0]), "misaligned:\n{screen}");
    }

    #[test]
    fn options_editor_and_groups_render() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let k = |c: KeyCode| KeyEvent::new(c, KeyModifiers::NONE);
        let mut app = app_for(sample_tree(), "catppuccin-mocha", IconMode::Emoji);
        app.on_key(k(KeyCode::Char('y')));
        let s = render(&mut app, 100, 18);
        println!("{s}");
        assert!(
            s.contains("─ 🦀 rust") || s.contains("─ 📁 directories"),
            "{s}"
        );
        assert!(s.contains("(by type)"));
        // Cursor never rests on a caption.
        assert!(app.is_selectable(app.cursor));

        app.on_key(k(KeyCode::Char('o')));
        let s = render(&mut app, 100, 24);
        println!("{s}");
        assert!(
            s.contains("Options")
                && s.contains("[x] group by type")
                && s.contains("[ ] directories first")
        );
        app.on_key(k(KeyCode::Esc));

        app.on_key(k(KeyCode::Char('T')));
        app.on_key(k(KeyCode::Char('e')));
        assert!(matches!(app.popup, crate::app::Popup::ThemeEditor(_)));
        let s = render(&mut app, 100, 30);
        println!("{s}");
        assert!(s.contains("Edit theme: Catppuccin Mocha (catppuccin-mocha)"));
        assert!(s.contains("Sample text 123"));
        assert!(s.contains("palette:"));
        // Edit dir fg via prompt and make sure it is live.
        for _ in 0..8 {
            app.on_key(k(KeyCode::Down));
        }
        // f opens the slider picker; # starts typed entry inside it.
        app.on_key(k(KeyCode::Char('f')));
        let s = render(&mut app, 100, 30);
        println!("{s}");
        assert!(s.contains("Colour: marker foreground"), "{s}");
        assert!(
            s.contains("▸ R") && s.contains("value mauve") && s.contains("preview"),
            "{s}"
        );
        for c in "#ff0000".chars() {
            app.on_key(k(KeyCode::Char(c)));
        }
        app.on_key(k(KeyCode::Enter)); // commit the typed value into the sliders
        let s = render(&mut app, 100, 30);
        assert!(s.contains("value #ff0000") && s.contains("255"), "{s}");
        // Sliders: nudge blue up, then apply.
        app.on_key(k(KeyCode::Down));
        app.on_key(k(KeyCode::Down));
        app.on_key(k(KeyCode::Right));
        let s = render(&mut app, 100, 30);
        assert!(s.contains("value #ff0001"), "{s}");
        app.on_key(k(KeyCode::Left));
        app.on_key(k(KeyCode::Enter)); // apply and close the picker
        let s = render(&mut app, 100, 30);
        assert!(s.contains("#ff0000"), "{s}");
        assert!(s.contains("▸marker       #ff0000"), "{s}");
        assert!(s.contains("Edit theme: Catppuccin Mocha (catppuccin-mocha) *"));
        // Esc once warns, Esc again discards and restores the theme.
        app.on_key(k(KeyCode::Esc));
        assert!(matches!(app.popup, crate::app::Popup::ThemeEditor(_)));
        app.on_key(k(KeyCode::Esc));
        assert!(matches!(app.popup, crate::app::Popup::None));
        assert_eq!(
            app.theme.styles.marker.fg,
            Some(ratatui::style::Color::Rgb(0xcb, 0xa6, 0xf7))
        );
    }

    #[test]
    fn key_guide_modes() {
        use crate::config::KeyGuide;
        let mut app = app_for(sample_tree(), "ansi", IconMode::Ascii);
        let full = render(&mut app, 80, 20);
        let guide_lines = full
            .lines()
            .filter(|l| l.contains("  move") || l.contains("  quit") || l.contains("  trash"))
            .count();
        assert!(guide_lines >= 2, "{full}");
        assert!(full.contains("Esc  quit"));
        // Tree view lists its own keys.
        app.on_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Tab,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        let tree = render(&mut app, 80, 20);
        assert!(
            tree.contains("*  expand all") && tree.contains("Tab  list view"),
            "{tree}"
        );
        // Compact: one line, truncated; Off: none.
        app.config.key_guide = KeyGuide::Compact;
        let compact = render(&mut app, 80, 20);
        assert!(
            compact.contains("↑↓ jk  move") && !compact.contains("Esc  quit"),
            "{compact}"
        );
        app.config.key_guide = KeyGuide::Off;
        let off = render(&mut app, 80, 20);
        assert!(!off.contains("↑↓ jk  move"));
        // A tiny terminal still keeps a list row and the status line.
        app.config.key_guide = KeyGuide::Full;
        let tiny = render(&mut app, 60, 6);
        assert!(tiny.contains("codu") && tiny.lines().count() == 6, "{tiny}");
    }

    #[test]
    fn volumes_screen_renders_and_mount_rows_show_volume_usage() {
        use crate::volumes::{Volume, VolumeKind};
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let k = |c: KeyCode| KeyEvent::new(c, KeyModifiers::NONE);
        let mut tree = sample_tree();
        tree.children[4].flags |= flags::OTHER_FS; // "secret" becomes a mount point
        let mut app = app_for(tree, "catppuccin-mocha", IconMode::Emoji);
        let backup = Volume {
            device: "/dev/sdb1".into(),
            mount_point: Some(PathBuf::from("/home/user/Documents/secret")),
            fs_type: "ext4".into(),
            label: Some("Backup".into()),
            model: None,
            total: 2_000_000_000_000,
            used: 1_200_000_000_000,
            kind: VolumeKind::Disk,
            read_only: false,
        };
        app.mounts = vec![backup.clone()];
        let s = render(&mut app, 100, 18);
        println!("{s}");
        assert!(s.contains("> 💾 secret"), "{s}");
        assert!(
            s.contains("1.1 TiB"),
            "mount row shows the volume's usage:\n{s}"
        );
        assert!(s.contains("volume sdb1"), "{s}");
        // A mount point without a known volume is still marked as one.
        app.mounts.clear();
        let s = render(&mut app, 100, 18);
        assert!(s.contains("mount point"), "{s}");
        assert!(s.contains("> 💾 secret"), "{s}");

        // V opens the volumes screen with an injected list.
        app.on_key(k(KeyCode::Char('V')));
        assert!(app.volumes.is_some());
        let view = app.volumes.as_mut().unwrap();
        view.list = vec![
            backup,
            Volume {
                device: "/dev/sdc2".into(),
                mount_point: None,
                fs_type: "ntfs".into(),
                label: Some("Games".into()),
                model: Some("Some SSD".into()),
                total: 500_000_000_000,
                used: 0,
                kind: VolumeKind::Removable,
                read_only: false,
            },
        ];
        view.cursor = 0;
        let s = render(&mut app, 110, 14);
        println!("{s}");
        assert!(s.contains("Volumes"));
        assert!(
            s.contains("💾 /home/user/Documents/se") && s.contains("sdb1") && s.contains("Backup"),
            "{s}"
        );
        assert!(
            s.contains("💤 /dev/sdc2") && s.contains("not mounted"),
            "{s}"
        );
        assert!(s.contains("2 volumes") && s.contains("1 mounted"));
        assert!(s.contains("scan volume"));
        // Enter on the unmounted one only explains; Esc returns to the browser.
        app.on_key(k(KeyCode::Down));
        app.on_key(k(KeyCode::Enter));
        assert!(app.volumes.is_some() && !app.is_scanning());
        assert!(app.status.as_ref().unwrap().text.contains("not mounted"));
        app.on_key(k(KeyCode::Esc));
        assert!(app.volumes.is_none() && !app.should_quit);
    }

    #[test]
    fn popups_render() {
        let mut app = app_for(sample_tree(), "dracula", IconMode::Emoji);
        app.popup = crate::app::Popup::Help { scroll: 0 };
        assert!(render(&mut app, 100, 40).contains("Help"));
        app.popup = crate::app::Popup::Info;
        let s = render(&mut app, 100, 40);
        assert!(
            s.contains("Info") && s.contains("/home/user/Documents/GitHub"),
            "{s}"
        );
        app.popup = crate::app::Popup::Confirm {
            mode: crate::app::DeleteMode::Trash,
            path: vec![0],
        };
        assert!(render(&mut app, 100, 40).contains("Move to trash"));
        app.popup = crate::app::Popup::Themes {
            cursor: 0,
            original: Box::new(app.theme.clone()),
            prompt: None,
        };
        assert!(render(&mut app, 100, 40).contains("Themes"));
        app.popup = crate::app::Popup::Message {
            title: "Oops".into(),
            body: "bad".into(),
            danger: true,
        };
        assert!(render(&mut app, 100, 40).contains("Oops"));
    }

    #[test]
    fn scanning_screen_renders() {
        let config = Config::default();
        let theme = Theme::load("tokyo-night").unwrap();
        let icons = IconSet::build(IconMode::Emoji, &theme.icons);
        let tmp = tempfile::tempdir().unwrap();
        let mut app = App::new(
            config,
            theme,
            icons,
            tmp.path().to_path_buf(),
            ScanOptions::default(),
            false,
        );
        let s = render(&mut app, 80, 20);
        // Either still scanning or already finished, both must render cleanly.
        assert!(s.contains("Scanning") || s.contains("codu"), "{s}");
        assert!(s.lines().all(|l| crate::format::width(l) <= 80));
    }
}
