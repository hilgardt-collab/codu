//! Application state and input handling.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use crate::config::{BarMode, Config, SortKey};
use crate::format;
use crate::icons::IconSet;
use crate::scan::{self, Kind, Node, Progress, ScanOptions};
use crate::theme::{self, Theme, ThemeInfo};

/// One row of the current listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The `..` entry.
    Parent,
    /// Index into the current directory's `children`.
    Entry(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteMode {
    Permanent,
    Trash,
}

#[derive(Debug)]
pub enum Popup {
    None,
    Help {
        scroll: u16,
    },
    Info,
    Confirm {
        mode: DeleteMode,
        child: usize,
    },
    Themes {
        cursor: usize,
        original: Box<Theme>,
    },
    Message {
        title: String,
        body: String,
        danger: bool,
    },
}

/// A scan running on a background thread.
pub struct ScanJob {
    pub progress: Arc<Progress>,
    pub rx: Receiver<io::Result<Node>>,
    pub started: Instant,
    pub path: PathBuf,
    /// Tree position the result replaces (empty = the whole tree).
    pub target: Vec<usize>,
}

/// A transient message for the status bar.
pub struct StatusMsg {
    pub text: String,
    pub warn: bool,
    pub until: Instant,
}

pub struct App {
    pub config: Config,
    pub theme: Theme,
    pub icons: IconSet,
    pub themes: Vec<ThemeInfo>,

    pub root_path: PathBuf,
    pub tree: Option<Node>,
    /// Child indices from the root down to the current directory.
    pub path: Vec<usize>,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub scroll: usize,

    pub sort: SortKey,
    pub sort_reverse: bool,
    pub dirs_first: bool,
    pub apparent: bool,
    pub si: bool,
    pub bar_mode: BarMode,
    pub bar_width: u16,
    pub show_count: bool,
    pub show_mtime: bool,
    pub show_hidden: bool,

    pub filter: String,
    pub filter_editing: bool,

    pub popup: Popup,
    pub scan: Option<ScanJob>,
    pub scan_options: Arc<ScanOptions>,
    pub last_scan: Option<Duration>,
    pub status: Option<StatusMsg>,
    pub read_only: bool,
    pub should_quit: bool,
    /// Set when the initial scan failed: quit once the error popup is dismissed.
    should_quit_on_dismiss: bool,

    /// Inner area of the list, recorded during drawing for mouse hit-testing.
    pub list_area: Rect,
    pub tick: u64,
    last_click: Option<(usize, Instant)>,
}

const STATUS_TTL: Duration = Duration::from_secs(4);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

impl App {
    pub fn new(
        config: Config,
        theme: Theme,
        icons: IconSet,
        root_path: PathBuf,
        scan_options: ScanOptions,
    ) -> App {
        let themes = theme::available_themes();
        let mut app = App {
            sort: config.sort,
            sort_reverse: config.sort_reverse,
            dirs_first: config.dirs_first,
            apparent: config.apparent_size,
            si: config.si,
            bar_mode: config.bar_mode,
            bar_width: config.bar_width.clamp(4, 60),
            show_count: config.show_count,
            show_mtime: config.show_mtime,
            show_hidden: config.show_hidden,
            read_only: config.read_only,
            config,
            theme,
            icons,
            themes,
            root_path,
            tree: None,
            path: Vec::new(),
            rows: Vec::new(),
            cursor: 0,
            scroll: 0,
            filter: String::new(),
            filter_editing: false,
            popup: Popup::None,
            scan: None,
            scan_options: Arc::new(scan_options),
            last_scan: None,
            status: None,
            should_quit: false,
            should_quit_on_dismiss: false,
            list_area: Rect::default(),
            tick: 0,
            last_click: None,
        };
        let root = app.root_path.clone();
        app.start_scan(root, Vec::new());
        app
    }

    // ------------------------------------------------------------------
    // Scanning

    fn start_scan(&mut self, path: PathBuf, target: Vec<usize>) {
        let progress = Arc::new(Progress::default());
        let (tx, rx) = mpsc::channel();
        let opts = Arc::clone(&self.scan_options);
        let p = Arc::clone(&progress);
        let scan_path = path.clone();
        std::thread::spawn(move || {
            let result = scan::scan(&scan_path, &opts, &p);
            let _ = tx.send(result);
        });
        self.scan = Some(ScanJob {
            progress,
            rx,
            started: Instant::now(),
            path,
            target,
        });
    }

    pub fn is_scanning(&self) -> bool {
        self.scan.is_some()
    }

    fn cancel_scan(&mut self) {
        if let Some(job) = self.scan.take() {
            job.progress
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn poll_scan(&mut self) {
        let Some(job) = &self.scan else { return };
        let result = match job.rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(io::Error::other("scanner thread exited unexpectedly"))
            }
        };
        let job = self.scan.take().expect("scan job present");
        self.last_scan = Some(job.started.elapsed());
        match result {
            Ok(node) => {
                if self.tree.is_some() {
                    self.splice(job.target, node);
                } else {
                    let items = node.items;
                    let size = node.size_of(self.apparent);
                    self.tree = Some(node);
                    self.path.clear();
                    self.rebuild_rows(None);
                    self.set_status(
                        format!(
                            "scanned {} items, {} in {}",
                            format::count(items),
                            format::bytes_str(size, self.si),
                            format::duration(self.last_scan.unwrap_or_default())
                        ),
                        false,
                    );
                }
            }
            Err(e) => {
                let title = if job.target.is_empty() {
                    "Scan failed"
                } else {
                    "Rescan failed"
                };
                self.popup = Popup::Message {
                    title: title.into(),
                    body: format!("{}\n\n{e}", job.path.display()),
                    danger: true,
                };
                if self.tree.is_none() {
                    // Nothing to browse: leave the popup up, quit on dismiss.
                    self.should_quit_on_dismiss = true;
                }
            }
        }
    }

    /// Replace the subtree at `target` with `node` and fix ancestor totals.
    fn splice(&mut self, target: Vec<usize>, mut node: Node) {
        let selected_name = self.selected_node().map(|n| n.name.to_string());
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        let old = node_at_mut(tree, &target);
        node.name = old.name.clone();
        let d_size = node.size as i128 - old.size as i128;
        let d_apparent = node.apparent as i128 - old.apparent as i128;
        let d_items = node.items as i128 - old.items as i128;
        *old = node;
        if let Some((_, ancestors)) = target.split_last() {
            adjust_ancestors(tree, ancestors, d_size, d_apparent, d_items);
        }
        self.rebuild_rows(None);
        if let Some(name) = selected_name {
            self.select_by_name(&name);
        }
        self.set_status(
            format!(
                "rescanned in {}",
                format::duration(self.last_scan.unwrap_or_default())
            ),
            false,
        );
    }

    // ------------------------------------------------------------------
    // Tree access

    pub fn current(&self) -> Option<&Node> {
        self.tree.as_ref().map(|t| node_at(t, &self.path))
    }

    /// Filesystem path of the current directory.
    pub fn current_path(&self) -> PathBuf {
        let mut p = self.root_path.clone();
        if let Some(tree) = &self.tree {
            let mut node = tree;
            for &i in &self.path {
                node = &node.children[i];
                p.push(&*node.name);
            }
        }
        p
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows.get(self.cursor).copied()
    }

    pub fn selected_child(&self) -> Option<usize> {
        match self.selected_row() {
            Some(Row::Entry(i)) => Some(i),
            _ => None,
        }
    }

    pub fn selected_node(&self) -> Option<&Node> {
        let i = self.selected_child()?;
        self.current().and_then(|d| d.children.get(i))
    }

    /// Largest entry in the current listing, for scaling bars.
    pub fn max_listed_size(&self) -> u64 {
        let Some(dir) = self.current() else { return 0 };
        self.rows
            .iter()
            .filter_map(|r| match r {
                Row::Entry(i) => Some(dir.children[*i].size_of(self.apparent)),
                Row::Parent => None,
            })
            .max()
            .unwrap_or(0)
    }

    /// Rebuild the sorted/filtered listing. If `keep` is given, the cursor is
    /// moved onto that child index; otherwise it stays put (clamped).
    pub fn rebuild_rows(&mut self, keep: Option<usize>) {
        let Some(dir) = self.current() else {
            self.rows.clear();
            self.cursor = 0;
            return;
        };
        let needle = self.filter.to_lowercase();
        let mut idx: Vec<usize> = (0..dir.children.len())
            .filter(|&i| {
                let c = &dir.children[i];
                (self.show_hidden || !c.name.starts_with('.'))
                    && (needle.is_empty() || c.name.to_lowercase().contains(&needle))
            })
            .collect();

        let key = self.sort;
        let reverse = self.sort_reverse;
        let dirs_first = self.dirs_first;
        let apparent = self.apparent;
        idx.sort_by(|&a, &b| {
            let (na, nb) = (&dir.children[a], &dir.children[b]);
            if dirs_first && na.is_dir() != nb.is_dir() {
                return nb.is_dir().cmp(&na.is_dir());
            }
            let ord = match key {
                SortKey::Size => nb.size_of(apparent).cmp(&na.size_of(apparent)),
                SortKey::Name => na.name.to_lowercase().cmp(&nb.name.to_lowercase()),
                SortKey::Count => nb.items.cmp(&na.items),
                SortKey::Mtime => nb.mtime.cmp(&na.mtime),
            };
            let ord = if reverse { ord.reverse() } else { ord };
            ord.then_with(|| na.name.to_lowercase().cmp(&nb.name.to_lowercase()))
        });

        self.rows.clear();
        if !self.path.is_empty() {
            self.rows.push(Row::Parent);
        }
        self.rows.extend(idx.into_iter().map(Row::Entry));

        if let Some(child) = keep
            && let Some(pos) = self.rows.iter().position(|r| *r == Row::Entry(child))
        {
            self.cursor = pos;
        }
        self.clamp_cursor();
    }

    fn select_by_name(&mut self, name: &str) {
        let Some(dir) = self.current() else { return };
        if let Some(pos) = self.rows.iter().position(|r| match r {
            Row::Entry(i) => &*dir.children[*i].name == name,
            Row::Parent => false,
        }) {
            self.cursor = pos;
        }
    }

    fn clamp_cursor(&mut self) {
        if self.rows.is_empty() {
            self.cursor = 0;
        } else if self.cursor >= self.rows.len() {
            self.cursor = self.rows.len() - 1;
        }
    }

    /// Keep the cursor inside the visible window of `height` rows.
    pub fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + height {
            self.scroll = self.cursor + 1 - height;
        }
        let max_scroll = self.rows.len().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll);
    }

    // ------------------------------------------------------------------
    // Navigation

    fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
    }

    fn page_size(&self) -> isize {
        (self.list_area.height as isize).max(1)
    }

    fn enter(&mut self) {
        match self.selected_row() {
            Some(Row::Parent) => self.go_parent(),
            Some(Row::Entry(i)) => {
                let Some(dir) = self.current() else { return };
                let child = &dir.children[i];
                if child.is_dir()
                    && !child.has(scan::flags::EXCLUDED)
                    && !child.has(scan::flags::OTHER_FS)
                {
                    self.path.push(i);
                    self.filter.clear();
                    self.filter_editing = false;
                    self.cursor = 0;
                    self.scroll = 0;
                    self.rebuild_rows(None);
                } else if child.is_dir() {
                    self.set_status("directory was not scanned".into(), true);
                }
            }
            None => {}
        }
    }

    fn go_parent(&mut self) {
        let Some(from) = self.path.pop() else { return };
        self.filter.clear();
        self.filter_editing = false;
        self.rebuild_rows(Some(from));
    }

    // ------------------------------------------------------------------
    // View toggles

    fn set_sort(&mut self, key: SortKey) {
        if self.sort == key {
            self.sort_reverse = !self.sort_reverse;
        } else {
            self.sort = key;
            self.sort_reverse = false;
        }
        let keep = self.selected_child();
        self.rebuild_rows(keep);
    }

    fn resort(&mut self) {
        let keep = self.selected_child();
        self.rebuild_rows(keep);
    }

    pub fn set_status(&mut self, text: String, warn: bool) {
        self.status = Some(StatusMsg {
            text,
            warn,
            until: Instant::now() + STATUS_TTL,
        });
    }

    pub fn apply_theme(&mut self, theme: Theme) {
        self.icons = IconSet::build(self.config.icons, &theme.icons);
        self.theme = theme;
    }

    fn preview_theme(&mut self, cursor: usize) {
        let Some(info) = self.themes.get(cursor) else {
            return;
        };
        let id = info.id.clone();
        match Theme::load(&id) {
            Ok(t) => {
                if !t.warnings.is_empty() {
                    self.set_status(format!("theme {}: {}", id, t.warnings.join("; ")), true);
                }
                self.apply_theme(t);
            }
            Err(e) => self.set_status(format!("theme {id}: {e:#}"), true),
        }
    }

    // ------------------------------------------------------------------
    // Actions

    fn request_delete(&mut self, mode: DeleteMode) {
        if self.read_only {
            self.set_status("read-only mode: deleting is disabled".into(), true);
            return;
        }
        let Some(child) = self.selected_child() else {
            return;
        };
        if self.config.confirm_delete {
            self.popup = Popup::Confirm { mode, child };
        } else {
            self.perform_delete(mode, child);
        }
    }

    fn perform_delete(&mut self, mode: DeleteMode, child: usize) {
        let Some(dir) = self.current() else { return };
        let Some(node) = dir.children.get(child) else {
            return;
        };
        let name = node.name.to_string();
        let kind = node.kind;
        let target = self.current_path().join(&name);

        let result: io::Result<()> = match mode {
            DeleteMode::Permanent => {
                if kind == Kind::Dir {
                    std::fs::remove_dir_all(&target)
                } else {
                    std::fs::remove_file(&target)
                }
            }
            DeleteMode::Trash => trash::delete(&target).map_err(io::Error::other),
        };

        match result {
            Ok(()) => {
                let Some(tree) = self.tree.as_mut() else {
                    return;
                };
                let dir = node_at_mut(tree, &self.path);
                let removed = dir.children.remove(child);
                let d_items = -(1 + removed.items as i128);
                adjust_ancestors(
                    tree,
                    &self.path,
                    -(removed.size as i128),
                    -(removed.apparent as i128),
                    d_items,
                );
                let cursor = self.cursor;
                self.rebuild_rows(None);
                self.cursor = cursor.min(self.rows.len().saturating_sub(1));
                let verb = if mode == DeleteMode::Trash {
                    "moved to trash"
                } else {
                    "deleted"
                };
                self.set_status(format!("{verb}: {name}"), false);
            }
            Err(e) => {
                self.popup = Popup::Message {
                    title: "Delete failed".into(),
                    body: format!("{}\n\n{e}", target.display()),
                    danger: true,
                };
            }
        }
    }

    fn refresh(&mut self) {
        if self.is_scanning() {
            return;
        }
        let path = self.current_path();
        let target = self.path.clone();
        self.start_scan(path, target);
    }

    // ------------------------------------------------------------------
    // Events

    pub fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if let Some(s) = &self.status
            && Instant::now() >= s.until
        {
            self.status = None;
        }
        self.poll_scan();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')) {
            self.should_quit = true;
            return;
        }

        if self.is_scanning() {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    if self.tree.is_none() {
                        self.should_quit = true;
                    } else {
                        self.cancel_scan();
                        self.set_status("rescan cancelled".into(), true);
                    }
                }
                _ => {}
            }
            return;
        }

        if !matches!(self.popup, Popup::None) {
            self.on_popup_key(key);
            return;
        }

        if self.filter_editing && self.on_filter_key(key) {
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.resort();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::PageUp => self.move_cursor(-self.page_size()),
            KeyCode::PageDown => self.move_cursor(self.page_size()),
            KeyCode::Char('u') if ctrl => self.move_cursor(-self.page_size() / 2),
            KeyCode::Char('d') if ctrl => self.move_cursor(self.page_size() / 2),
            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => self.cursor = self.rows.len().saturating_sub(1),
            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => self.enter(),
            KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => self.go_parent(),

            KeyCode::Char('s') => self.set_sort(SortKey::Size),
            KeyCode::Char('n') => self.set_sort(SortKey::Name),
            KeyCode::Char('C') => self.set_sort(SortKey::Count),
            KeyCode::Char('M') => self.set_sort(SortKey::Mtime),
            KeyCode::Char('t') => {
                self.dirs_first = !self.dirs_first;
                self.resort();
            }
            KeyCode::Char('a') => {
                self.apparent = !self.apparent;
                self.resort();
            }
            KeyCode::Char('b') => self.bar_mode = self.bar_mode.next(),
            KeyCode::Char('c') => self.show_count = !self.show_count,
            KeyCode::Char('m') => self.show_mtime = !self.show_mtime,
            KeyCode::Char('e') => {
                self.show_hidden = !self.show_hidden;
                self.resort();
            }
            KeyCode::Char('/') => {
                self.filter_editing = true;
            }
            KeyCode::Char('i') => {
                if self.selected_child().is_some() {
                    self.popup = Popup::Info;
                }
            }
            KeyCode::Char('d') => self.request_delete(DeleteMode::Permanent),
            KeyCode::Char('D') => self.request_delete(DeleteMode::Trash),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('T') => {
                let cursor = self
                    .themes
                    .iter()
                    .position(|t| t.id == self.theme.id)
                    .unwrap_or(0);
                self.popup = Popup::Themes {
                    cursor,
                    original: Box::new(self.theme.clone()),
                };
            }
            KeyCode::Char('?') | KeyCode::F(1) => self.popup = Popup::Help { scroll: 0 },
            _ => {}
        }
    }

    /// Returns true when the key was consumed by the filter editor.
    fn on_filter_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.filter_editing = false;
                self.resort();
                true
            }
            KeyCode::Enter => {
                self.filter_editing = false;
                true
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.resort();
                true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.resort();
                self.cursor = if self.path.is_empty() {
                    0
                } else {
                    1.min(self.rows.len().saturating_sub(1))
                };
                true
            }
            _ => false,
        }
    }

    fn on_popup_key(&mut self, key: KeyEvent) {
        match &mut self.popup {
            Popup::None => {}
            Popup::Help { scroll } => match key.code {
                KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => *scroll = scroll.saturating_add(10),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
                _ => self.popup = Popup::None,
            },
            Popup::Info => self.popup = Popup::None,
            Popup::Confirm { mode, child } => {
                let (mode, child) = (*mode, *child);
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.popup = Popup::None;
                        self.perform_delete(mode, child);
                    }
                    _ => self.popup = Popup::None,
                }
            }
            Popup::Themes { cursor, original } => {
                let n = self.themes.len();
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') if n > 0 => {
                        let c = (*cursor + 1) % n;
                        *cursor = c;
                        self.preview_theme(c);
                    }
                    KeyCode::Up | KeyCode::Char('k') if n > 0 => {
                        let c = (*cursor + n - 1) % n;
                        *cursor = c;
                        self.preview_theme(c);
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        let id = self.theme.id.clone();
                        self.popup = Popup::None;
                        self.set_status(
                            format!(
                                "theme: {id}  (add theme = \"{id}\" to config.toml to keep it)"
                            ),
                            false,
                        );
                    }
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('T') => {
                        let original = std::mem::replace(original, Box::new(Theme::base()));
                        self.popup = Popup::None;
                        self.apply_theme(*original);
                    }
                    _ => {}
                }
            }
            Popup::Message { .. } => {
                self.popup = Popup::None;
                if self.should_quit_on_dismiss {
                    self.should_quit = true;
                }
            }
        }
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.is_scanning() || !matches!(self.popup, Popup::None) {
            return;
        }
        match ev.kind {
            MouseEventKind::ScrollDown => self.move_cursor(3),
            MouseEventKind::ScrollUp => self.move_cursor(-3),
            MouseEventKind::Down(MouseButton::Left) => {
                let a = self.list_area;
                if ev.column < a.x
                    || ev.column >= a.x + a.width
                    || ev.row < a.y
                    || ev.row >= a.y + a.height
                {
                    return;
                }
                let row = self.scroll + (ev.row - a.y) as usize;
                if row >= self.rows.len() {
                    return;
                }
                let now = Instant::now();
                let double = matches!(self.last_click, Some((r, t)) if r == row && now.duration_since(t) < DOUBLE_CLICK);
                self.cursor = row;
                if double {
                    self.last_click = None;
                    self.enter();
                } else {
                    self.last_click = Some((row, now));
                }
            }
            _ => {}
        }
    }
}

// ----------------------------------------------------------------------
// Tree helpers

pub fn node_at<'a>(root: &'a Node, path: &[usize]) -> &'a Node {
    path.iter().fold(root, |n, &i| &n.children[i])
}

pub fn node_at_mut<'a>(root: &'a mut Node, path: &[usize]) -> &'a mut Node {
    path.iter().fold(root, |n, &i| &mut n.children[i])
}

/// Apply size/item deltas to the root and every node along `path` (inclusive).
fn adjust_ancestors(
    root: &mut Node,
    path: &[usize],
    d_size: i128,
    d_apparent: i128,
    d_items: i128,
) {
    let apply = |n: &mut Node| {
        n.size = (n.size as i128 + d_size).max(0) as u64;
        n.apparent = (n.apparent as i128 + d_apparent).max(0) as u64;
        n.items = (n.items as i128 + d_items).max(0) as u64;
    };
    let mut node = root;
    for &i in path {
        apply(node);
        node = &mut node.children[i];
    }
    apply(node);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str, size: u64) -> Node {
        Node {
            name: name.into(),
            kind: Kind::File,
            size,
            apparent: size,
            mtime: 0,
            items: 0,
            flags: 0,
            children: vec![],
        }
    }

    fn dir(name: &str, children: Vec<Node>) -> Node {
        let size = children.iter().map(|c| c.size).sum();
        let items = children.iter().map(|c| 1 + c.items).sum();
        Node {
            name: name.into(),
            kind: Kind::Dir,
            size,
            apparent: size,
            mtime: 0,
            items,
            flags: 0,
            children,
        }
    }

    fn app_with(tree: Node) -> App {
        let cfg = Config::default();
        let theme = Theme::base();
        let icons = IconSet::build(crate::icons::IconMode::None, &theme.icons);
        let mut app = App {
            sort: cfg.sort,
            sort_reverse: false,
            dirs_first: false,
            apparent: false,
            si: false,
            bar_mode: cfg.bar_mode,
            bar_width: 20,
            show_count: false,
            show_mtime: false,
            show_hidden: true,
            read_only: false,
            config: cfg,
            theme,
            icons,
            themes: vec![],
            root_path: PathBuf::from("/root"),
            tree: Some(tree),
            path: vec![],
            rows: vec![],
            cursor: 0,
            scroll: 0,
            filter: String::new(),
            filter_editing: false,
            popup: Popup::None,
            scan: None,
            scan_options: Arc::new(ScanOptions::default()),
            last_scan: None,
            status: None,
            should_quit: false,
            list_area: Rect::new(0, 0, 80, 10),
            tick: 0,
            last_click: None,
            should_quit_on_dismiss: false,
        };
        app.rebuild_rows(None);
        app
    }

    fn sample() -> Node {
        dir(
            "/root",
            vec![
                leaf("small", 10),
                dir("big", vec![leaf("x", 500), leaf("y", 500)]),
                leaf(".hidden", 300),
                leaf("medium", 100),
            ],
        )
    }

    fn names(app: &App) -> Vec<String> {
        let d = app.current().unwrap();
        app.rows
            .iter()
            .map(|r| match r {
                Row::Parent => "..".to_string(),
                Row::Entry(i) => d.children[*i].name.to_string(),
            })
            .collect()
    }

    #[test]
    fn sorts_by_size_descending_by_default() {
        let app = app_with(sample());
        assert_eq!(names(&app), ["big", ".hidden", "medium", "small"]);
    }

    #[test]
    fn sort_toggles_reverse_and_name() {
        let mut app = app_with(sample());
        app.set_sort(SortKey::Size);
        assert!(app.sort_reverse);
        assert_eq!(names(&app), ["small", "medium", ".hidden", "big"]);
        app.set_sort(SortKey::Name);
        assert!(!app.sort_reverse);
        assert_eq!(names(&app), [".hidden", "big", "medium", "small"]);
    }

    #[test]
    fn dirs_first_and_hidden_toggle() {
        let mut app = app_with(sample());
        app.dirs_first = true;
        app.show_hidden = false;
        app.rebuild_rows(None);
        assert_eq!(names(&app), ["big", "medium", "small"]);
    }

    #[test]
    fn enter_and_parent_restore_selection() {
        let mut app = app_with(sample());
        assert_eq!(app.cursor, 0); // "big"
        app.enter();
        assert_eq!(app.path, vec![1]);
        assert_eq!(names(&app), ["..", "x", "y"]);
        assert_eq!(app.current_path(), PathBuf::from("/root/big"));
        app.go_parent();
        assert!(app.path.is_empty());
        assert_eq!(names(&app)[app.cursor], "big");
    }

    #[test]
    fn filter_narrows_listing() {
        let mut app = app_with(sample());
        app.filter = "m".into();
        app.rebuild_rows(None);
        assert_eq!(names(&app), ["medium", "small"]);
    }

    #[test]
    fn ensure_visible_scrolls() {
        let mut app = app_with(sample());
        app.cursor = 3;
        app.ensure_visible(2);
        assert_eq!(app.scroll, 2);
        app.cursor = 0;
        app.ensure_visible(2);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn splice_adjusts_ancestors() {
        let mut app = app_with(sample());
        app.enter(); // into big (1000 bytes, 2 items)
        let replacement = dir("ignored", vec![leaf("z", 100)]);
        app.splice(vec![1], replacement);
        let root = app.tree.as_ref().unwrap();
        assert_eq!(root.size, 10 + 100 + 300 + 100);
        assert_eq!(root.items, 4 + 1);
        assert_eq!(&*root.children[1].name, "big");
        assert_eq!(names(&app), ["..", "z"]);
    }

    #[test]
    fn adjust_ancestors_walks_path() {
        let mut root = sample();
        adjust_ancestors(&mut root, &[1], -500, -500, -1);
        assert_eq!(root.size, 1410 - 500);
        assert_eq!(root.children[1].size, 500);
        assert_eq!(root.children[1].items, 1);
        assert_eq!(root.children[0].size, 10);
    }
}
