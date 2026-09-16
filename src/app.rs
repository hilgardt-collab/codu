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

use crate::config::{BarMode, Config, SortKey, View};
use crate::format;
use crate::icons::IconSet;
use crate::scan::{self, Kind, Node, Progress, ScanOptions};
use crate::theme::{self, Theme, ThemeInfo};

/// One row of the list view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The `..` entry.
    Parent,
    /// Index into the current directory's `children`.
    Entry(usize),
}

/// One row of the tree view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// Child indices from the root; empty for the root row.
    pub path: Vec<usize>,
    /// One entry per ancestor level between the root and this node's parent:
    /// `true` when that ancestor was the last of its siblings (no guide line).
    pub guides: Vec<bool>,
    /// Whether this node is the last of its siblings.
    pub is_last: bool,
    /// The `..` row above the root.
    pub parent: bool,
}

impl TreeRow {
    pub fn depth(&self) -> usize {
        self.path.len()
    }
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
        path: Vec<usize>,
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
    /// Set when walking up: the scanned path becomes the new root.
    pub new_root: Option<PathBuf>,
    /// Entry to select once the scan lands (used when walking up).
    pub select: Option<String>,
}

/// A transient message for the status bar.
pub struct StatusMsg {
    pub text: String,
    pub warn: bool,
    pub until: Instant,
}

/// Sorting/filtering parameters shared by both views.
struct ListOpts {
    sort: SortKey,
    reverse: bool,
    dirs_first: bool,
    apparent: bool,
    show_hidden: bool,
    needle: String,
    /// Tree view: directories are listed even when they don't match the filter.
    dirs_always: bool,
}

pub struct App {
    pub config: Config,
    pub theme: Theme,
    pub icons: IconSet,
    pub themes: Vec<ThemeInfo>,

    pub root_path: PathBuf,
    pub tree: Option<Node>,
    pub view: View,
    /// List view: child indices from the root down to the current directory.
    pub path: Vec<usize>,
    pub rows: Vec<Row>,
    pub tree_rows: Vec<TreeRow>,
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
            view: config.view,
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
            tree_rows: Vec::new(),
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
        app.start_scan(root, Vec::new(), None, None);
        app
    }

    // ------------------------------------------------------------------
    // Scanning

    fn start_scan(
        &mut self,
        path: PathBuf,
        target: Vec<usize>,
        new_root: Option<PathBuf>,
        select: Option<String>,
    ) {
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
            new_root,
            select,
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
            Ok(mut node) => {
                if let Some(new_root) = job.new_root {
                    node.expanded = true;
                    self.root_path = new_root;
                    self.tree = Some(node);
                    self.path.clear();
                    self.rebuild();
                    if let Some(name) = &job.select {
                        self.select_child_by_name(name);
                    }
                    self.set_status(
                        format!(
                            "scanned parent in {}",
                            format::duration(self.last_scan.unwrap_or_default())
                        ),
                        false,
                    );
                } else if self.tree.is_some() {
                    self.splice(job.target, node);
                } else {
                    let items = node.items;
                    let size = node.size_of(self.apparent);
                    node.expanded = true;
                    self.tree = Some(node);
                    self.path.clear();
                    self.rebuild();
                    self.select_first_entry();
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
                    self.should_quit_on_dismiss = true;
                }
            }
        }
    }

    /// Replace the subtree at `target` with `node` and fix ancestor totals.
    fn splice(&mut self, target: Vec<usize>, mut node: Node) {
        let selected = self.selected_path();
        let selected_name = self.selected_node().map(|n| n.name.to_string());
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        let old = node_at_mut(tree, &target);
        node.name = old.name.clone();
        node.expanded = old.expanded;
        let d_size = node.size as i128 - old.size as i128;
        let d_apparent = node.apparent as i128 - old.apparent as i128;
        let d_items = node.items as i128 - old.items as i128;
        *old = node;
        if let Some((_, ancestors)) = target.split_last() {
            adjust_ancestors(tree, ancestors, d_size, d_apparent, d_items);
        }
        match self.view {
            View::List => {
                self.rebuild_rows(None);
                if let Some(name) = selected_name {
                    self.select_child_by_name(&name);
                }
            }
            View::Tree => {
                // A selection inside the rescanned subtree no longer exists.
                let keep = match selected {
                    Some(p) if p.starts_with(&target) && p != target => target,
                    Some(p) => p,
                    None => Vec::new(),
                };
                self.rebuild_tree_rows(Some(&keep));
            }
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

    /// Filesystem path of the node at `path`.
    pub fn fs_path(&self, path: &[usize]) -> PathBuf {
        let mut p = self.root_path.clone();
        if let Some(tree) = &self.tree {
            let mut node = tree;
            for &i in path {
                node = &node.children[i];
                p.push(&*node.name);
            }
        }
        p
    }

    /// Filesystem path of the directory shown in list view.
    pub fn current_path(&self) -> PathBuf {
        self.fs_path(&self.path)
    }

    /// Path shown in the header for the active view.
    pub fn header_path(&self) -> PathBuf {
        match self.view {
            View::List => self.current_path(),
            View::Tree => self.root_path.clone(),
        }
    }

    pub fn row_count(&self) -> usize {
        match self.view {
            View::List => self.rows.len(),
            View::Tree => self.tree_rows.len(),
        }
    }

    pub fn selected_row(&self) -> Option<Row> {
        self.rows.get(self.cursor).copied()
    }

    pub fn selected_tree_row(&self) -> Option<&TreeRow> {
        self.tree_rows.get(self.cursor)
    }

    /// Tree path of the selected entry; `None` for the `..` row.
    pub fn selected_path(&self) -> Option<Vec<usize>> {
        match self.view {
            View::List => match self.selected_row() {
                Some(Row::Entry(i)) => {
                    let mut p = self.path.clone();
                    p.push(i);
                    Some(p)
                }
                _ => None,
            },
            View::Tree => self
                .selected_tree_row()
                .filter(|r| !r.parent)
                .map(|r| r.path.clone()),
        }
    }

    pub fn selected_node(&self) -> Option<&Node> {
        let path = self.selected_path()?;
        self.tree.as_ref().map(|t| node_at(t, &path))
    }

    /// Size the selected entry's share is measured against.
    pub fn selected_share_base(&self) -> u64 {
        let base = match (self.view, self.selected_path()) {
            (View::Tree, Some(p)) if !p.is_empty() => {
                self.tree.as_ref().map(|t| node_at(t, &p[..p.len() - 1]))
            }
            (View::Tree, _) => self.tree.as_ref(),
            (View::List, _) => self.current(),
        };
        base.map(|n| n.size_of(self.apparent)).unwrap_or(0)
    }

    /// Largest entry in the list view, for scaling bars.
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

    fn list_opts(&self, dirs_always: bool) -> ListOpts {
        ListOpts {
            sort: self.sort,
            reverse: self.sort_reverse,
            dirs_first: self.dirs_first,
            apparent: self.apparent,
            show_hidden: self.show_hidden,
            needle: self.filter.to_lowercase(),
            dirs_always,
        }
    }

    /// Rebuild the rows of whichever view is active, keeping the selection.
    pub fn rebuild(&mut self) {
        match self.view {
            View::List => {
                let keep = match self.selected_row() {
                    Some(Row::Entry(i)) => Some(i),
                    _ => None,
                };
                self.rebuild_rows(keep);
            }
            View::Tree => {
                let keep = self.selected_path();
                self.rebuild_tree_rows(keep.as_deref());
            }
        }
    }

    /// Rebuild the list view. If `keep` is given, the cursor is moved onto
    /// that child index; otherwise it stays put (clamped).
    pub fn rebuild_rows(&mut self, keep: Option<usize>) {
        let opts = self.list_opts(false);
        let Some(dir) = self.current() else {
            self.rows.clear();
            self.cursor = 0;
            return;
        };
        let idx = sorted_children(dir, &opts);
        self.rows.clear();
        self.rows.push(Row::Parent);
        self.rows.extend(idx.into_iter().map(Row::Entry));

        if let Some(child) = keep
            && let Some(pos) = self.rows.iter().position(|r| *r == Row::Entry(child))
        {
            self.cursor = pos;
        }
        self.clamp_cursor();
    }

    /// Rebuild the tree view. If `keep` is given, the cursor is moved onto
    /// that path (or the nearest surviving ancestor).
    pub fn rebuild_tree_rows(&mut self, keep: Option<&[usize]>) {
        let opts = self.list_opts(true);
        let Some(tree) = &self.tree else {
            self.tree_rows.clear();
            self.cursor = 0;
            return;
        };
        let mut rows = vec![TreeRow {
            path: vec![],
            guides: vec![],
            is_last: true,
            parent: true,
        }];
        let mut path = Vec::new();
        flatten(tree, &mut path, &[], true, &opts, &mut rows);
        self.tree_rows = rows;

        if let Some(keep) = keep {
            let mut want = keep;
            loop {
                if let Some(pos) = self
                    .tree_rows
                    .iter()
                    .position(|r| !r.parent && r.path == want)
                {
                    self.cursor = pos;
                    break;
                }
                match want.split_last() {
                    Some((_, rest)) => want = rest,
                    None => break,
                }
            }
        }
        self.clamp_cursor();
    }

    /// Select the root's child called `name` in the active view.
    fn select_child_by_name(&mut self, name: &str) {
        let Some(tree) = &self.tree else { return };
        match self.view {
            View::List => {
                let dir = node_at(tree, &self.path);
                if let Some(pos) = self.rows.iter().position(|r| match r {
                    Row::Entry(i) => &*dir.children[*i].name == name,
                    Row::Parent => false,
                }) {
                    self.cursor = pos;
                }
            }
            View::Tree => {
                if let Some(pos) = self.tree_rows.iter().position(|r| {
                    !r.parent && r.path.len() == 1 && &*tree.children[r.path[0]].name == name
                }) {
                    self.cursor = pos;
                }
            }
        }
    }

    /// Put the cursor on the first real entry rather than `..`.
    fn select_first_entry(&mut self) {
        self.cursor = if self.row_count() > 1 { 1 } else { 0 };
    }

    fn clamp_cursor(&mut self) {
        let n = self.row_count();
        if n == 0 {
            self.cursor = 0;
        } else if self.cursor >= n {
            self.cursor = n - 1;
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
        let max_scroll = self.row_count().saturating_sub(height);
        self.scroll = self.scroll.min(max_scroll);
    }

    // ------------------------------------------------------------------
    // Navigation

    fn move_cursor(&mut self, delta: isize) {
        let n = self.row_count();
        if n == 0 {
            return;
        }
        self.cursor = (self.cursor as isize + delta).clamp(0, n as isize - 1) as usize;
    }

    fn page_size(&self) -> isize {
        (self.list_area.height as isize).max(1)
    }

    /// Enter / → in list view.
    fn enter(&mut self) {
        match self.selected_row() {
            Some(Row::Parent) => {
                if self.path.is_empty() {
                    self.walk_up();
                } else {
                    self.go_parent();
                }
            }
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
                    self.select_first_entry();
                } else if child.is_dir() {
                    self.set_status("directory was not scanned".into(), true);
                }
            }
            None => {}
        }
    }

    fn go_parent(&mut self) {
        let Some(from) = self.path.pop() else {
            self.set_status(
                "at the scanned root: open .. to scan the parent directory".into(),
                true,
            );
            return;
        };
        self.filter.clear();
        self.filter_editing = false;
        self.rebuild_rows(Some(from));
    }

    /// Scan the parent of the current root and make it the new root.
    fn walk_up(&mut self) {
        if self.is_scanning() {
            return;
        }
        let Some(parent) = self.root_path.parent().map(|p| p.to_path_buf()) else {
            self.set_status("already at the filesystem root".into(), true);
            return;
        };
        let name = self
            .root_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.filter.clear();
        self.filter_editing = false;
        self.start_scan(parent.clone(), Vec::new(), Some(parent), Some(name));
    }

    // ------------------------------------------------------------------
    // Tree view operations

    fn tree_node_mut(&mut self, path: &[usize]) -> Option<&mut Node> {
        self.tree.as_mut().map(|t| node_at_mut(t, path))
    }

    /// Expand or collapse the selected directory. Returns true if it changed.
    fn tree_set_expanded(&mut self, expanded: bool) -> bool {
        let Some(path) = self.selected_path() else {
            return false;
        };
        let Some(node) = self.tree_node_mut(&path) else {
            return false;
        };
        if !node.is_dir() || node.expanded == expanded {
            return false;
        }
        node.expanded = expanded;
        self.rebuild_tree_rows(Some(&path));
        true
    }

    fn tree_toggle(&mut self) {
        match self.selected_tree_row() {
            Some(r) if r.parent => self.walk_up(),
            Some(_) => {
                let expanded = self.selected_node().map(|n| n.expanded).unwrap_or(false);
                self.tree_set_expanded(!expanded);
            }
            None => {}
        }
    }

    fn tree_expand_all(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        if let Some(node) = self.tree_node_mut(&path) {
            node.set_expanded_recursive(true);
        }
        self.rebuild_tree_rows(Some(&path));
    }

    /// → in tree view: expand, or step onto the first child if already open.
    fn tree_expand_or_child(&mut self) {
        if self.tree_set_expanded(true) {
            return;
        }
        let Some(path) = self.selected_path() else {
            return;
        };
        if let Some(next) = self.tree_rows.get(self.cursor + 1)
            && next.path.len() == path.len() + 1
        {
            self.cursor += 1;
        }
    }

    /// ← in tree view: collapse, or jump to the parent if already closed.
    fn tree_collapse_or_parent(&mut self) {
        if self.tree_set_expanded(false) {
            return;
        }
        self.tree_parent_row();
    }

    fn tree_parent_row(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        let Some((_, parent)) = path.split_last() else {
            return;
        };
        if let Some(pos) = self
            .tree_rows
            .iter()
            .position(|r| !r.parent && r.path == parent)
        {
            self.cursor = pos;
        }
    }

    /// Switch between list and tree view, keeping the selection.
    fn switch_view(&mut self) {
        match self.view {
            View::List => {
                let selected = self.selected_path();
                if let Some(tree) = self.tree.as_mut() {
                    let mut node = tree;
                    node.expanded = true;
                    for &i in &self.path {
                        node = &mut node.children[i];
                        node.expanded = true;
                    }
                }
                self.view = View::Tree;
                let keep = selected.unwrap_or_else(|| self.path.clone());
                self.rebuild_tree_rows(Some(&keep));
            }
            View::Tree => {
                let selected = self.selected_path();
                self.view = View::List;
                match selected.as_deref().and_then(|p| p.split_last()) {
                    Some((child, parent)) => {
                        self.path = parent.to_vec();
                        self.rebuild_rows(Some(*child));
                    }
                    None => {
                        self.path.clear();
                        self.rebuild_rows(None);
                        self.select_first_entry();
                    }
                }
            }
        }
        self.filter_editing = false;
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
        self.rebuild();
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
        let Some(path) = self.selected_path() else {
            return;
        };
        if path.is_empty() {
            self.set_status("cannot delete the scanned root".into(), true);
            return;
        }
        if self.config.confirm_delete {
            self.popup = Popup::Confirm { mode, path };
        } else {
            self.perform_delete(mode, path);
        }
    }

    fn perform_delete(&mut self, mode: DeleteMode, path: Vec<usize>) {
        let Some((&child, parent_path)) = path.split_last() else {
            return;
        };
        let Some(tree) = &self.tree else { return };
        let Some(node) = node_at(tree, parent_path).children.get(child) else {
            return;
        };
        let name = node.name.to_string();
        let kind = node.kind;
        let target = self.fs_path(&path);

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
                let removed = node_at_mut(tree, parent_path).children.remove(child);
                adjust_ancestors(
                    tree,
                    parent_path,
                    -(removed.size as i128),
                    -(removed.apparent as i128),
                    -(1 + removed.items as i128),
                );
                fix_path_after_delete(&mut self.path, &path);
                let cursor = self.cursor;
                match self.view {
                    View::List => self.rebuild_rows(None),
                    View::Tree => self.rebuild_tree_rows(None),
                }
                self.cursor = cursor.min(self.row_count().saturating_sub(1));
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

    /// Rescan: the current directory in list view, the selected directory
    /// (or its parent) in tree view.
    fn refresh(&mut self) {
        if self.is_scanning() {
            return;
        }
        let target = match self.view {
            View::List => self.path.clone(),
            View::Tree => match self.selected_path() {
                Some(p) => {
                    let is_dir = self.selected_node().is_some_and(|n| n.is_dir());
                    if is_dir { p } else { p[..p.len() - 1].to_vec() }
                }
                None => Vec::new(),
            },
        };
        let path = self.fs_path(&target);
        self.start_scan(path, target, None, None);
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
            if key.code == KeyCode::Esc {
                if self.tree.is_none() {
                    self.should_quit = true;
                } else {
                    self.cancel_scan();
                    self.set_status("scan cancelled".into(), true);
                }
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

        let tree = self.view == View::Tree;
        match key.code {
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.rebuild();
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::PageUp => self.move_cursor(-self.page_size()),
            KeyCode::PageDown => self.move_cursor(self.page_size()),
            KeyCode::Char('u') if ctrl => self.move_cursor(-self.page_size() / 2),
            KeyCode::Char('d') if ctrl => self.move_cursor(self.page_size() / 2),
            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => self.cursor = self.row_count().saturating_sub(1),
            KeyCode::Tab | KeyCode::Char('v') => self.switch_view(),

            KeyCode::Enter if tree => self.tree_toggle(),
            KeyCode::Char(' ') if tree => self.tree_toggle(),
            KeyCode::Right | KeyCode::Char('l') if tree => self.tree_expand_or_child(),
            KeyCode::Left | KeyCode::Char('h') if tree => self.tree_collapse_or_parent(),
            KeyCode::Backspace if tree => self.tree_parent_row(),
            KeyCode::Char('+') | KeyCode::Char('=') if tree => {
                self.tree_set_expanded(true);
            }
            KeyCode::Char('-') if tree => {
                self.tree_set_expanded(false);
            }
            KeyCode::Char('*') if tree => self.tree_expand_all(),

            KeyCode::Right | KeyCode::Enter | KeyCode::Char('l') => self.enter(),
            KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => self.go_parent(),

            KeyCode::Char('s') => self.set_sort(SortKey::Size),
            KeyCode::Char('n') => self.set_sort(SortKey::Name),
            KeyCode::Char('C') => self.set_sort(SortKey::Count),
            KeyCode::Char('M') => self.set_sort(SortKey::Mtime),
            KeyCode::Char('t') => {
                self.dirs_first = !self.dirs_first;
                self.rebuild();
            }
            KeyCode::Char('a') => {
                self.apparent = !self.apparent;
                self.rebuild();
            }
            KeyCode::Char('b') => self.bar_mode = self.bar_mode.next(),
            KeyCode::Char('c') => self.show_count = !self.show_count,
            KeyCode::Char('m') => self.show_mtime = !self.show_mtime,
            KeyCode::Char('e') => {
                self.show_hidden = !self.show_hidden;
                self.rebuild();
            }
            KeyCode::Char('/') => self.filter_editing = true,
            KeyCode::Char('i') => {
                if self.selected_path().is_some() {
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
                self.rebuild();
                true
            }
            KeyCode::Enter => {
                self.filter_editing = false;
                true
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.rebuild();
                true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.push(c);
                self.rebuild();
                if self.view == View::List {
                    self.select_first_entry();
                }
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
            Popup::Confirm { mode, path } => {
                let (mode, path) = (*mode, std::mem::take(path));
                self.popup = Popup::None;
                if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                    self.perform_delete(mode, path);
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
                    KeyCode::Esc | KeyCode::Char('T') => {
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
                if row >= self.row_count() {
                    return;
                }
                let now = Instant::now();
                let double = matches!(self.last_click, Some((r, t)) if r == row && now.duration_since(t) < DOUBLE_CLICK);
                self.cursor = row;
                if double {
                    self.last_click = None;
                    match self.view {
                        View::List => self.enter(),
                        View::Tree => self.tree_toggle(),
                    }
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

/// Sorted, filtered child indices of `dir`.
fn sorted_children(dir: &Node, o: &ListOpts) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..dir.children.len())
        .filter(|&i| {
            let c = &dir.children[i];
            (o.show_hidden || !c.name.starts_with('.'))
                && (o.needle.is_empty()
                    || (o.dirs_always && c.is_dir())
                    || c.name.to_lowercase().contains(&o.needle))
        })
        .collect();
    idx.sort_by(|&a, &b| {
        let (na, nb) = (&dir.children[a], &dir.children[b]);
        if o.dirs_first && na.is_dir() != nb.is_dir() {
            return nb.is_dir().cmp(&na.is_dir());
        }
        let ord = match o.sort {
            SortKey::Size => nb.size_of(o.apparent).cmp(&na.size_of(o.apparent)),
            SortKey::Name => na.name.to_lowercase().cmp(&nb.name.to_lowercase()),
            SortKey::Count => nb.items.cmp(&na.items),
            SortKey::Mtime => nb.mtime.cmp(&na.mtime),
        };
        let ord = if o.reverse { ord.reverse() } else { ord };
        ord.then_with(|| na.name.to_lowercase().cmp(&nb.name.to_lowercase()))
    });
    idx
}

/// Depth-first flattening of the expanded tree into rows.
fn flatten(
    node: &Node,
    path: &mut Vec<usize>,
    guides: &[bool],
    is_last: bool,
    o: &ListOpts,
    out: &mut Vec<TreeRow>,
) {
    out.push(TreeRow {
        path: path.clone(),
        guides: guides.to_vec(),
        is_last,
        parent: false,
    });
    if !(node.is_dir() && node.expanded) {
        return;
    }
    let kids = sorted_children(node, o);
    let n = kids.len();
    let child_guides: Vec<bool> = if path.is_empty() {
        Vec::new()
    } else {
        let mut g = guides.to_vec();
        g.push(is_last);
        g
    };
    for (k, &i) in kids.iter().enumerate() {
        path.push(i);
        flatten(&node.children[i], path, &child_guides, k + 1 == n, o, out);
        path.pop();
    }
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

/// Keep a list-view path valid after the node at `deleted` was removed:
/// sibling indices after it shift down, and a path through it is cut short.
fn fix_path_after_delete(current: &mut Vec<usize>, deleted: &[usize]) {
    let Some((&last, prefix)) = deleted.split_last() else {
        return;
    };
    if current.len() > prefix.len() && current[..prefix.len()] == *prefix {
        let idx = prefix.len();
        if current[idx] == last {
            current.truncate(idx);
        } else if current[idx] > last {
            current[idx] -= 1;
        }
    }
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
            expanded: false,
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
            expanded: false,
            children,
        }
    }

    fn app_with(tree: Node) -> App {
        let cfg = Config::default();
        let theme = Theme::base();
        let icons = IconSet::build(crate::icons::IconMode::None, &theme.icons);
        let mut app = App {
            view: View::List,
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
            tree_rows: vec![],
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
            should_quit_on_dismiss: false,
            list_area: Rect::new(0, 0, 80, 10),
            tick: 0,
            last_click: None,
        };
        if let Some(t) = app.tree.as_mut() {
            t.expanded = true;
        }
        app.rebuild_rows(None);
        app.select_first_entry();
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

    fn tree_names(app: &App) -> Vec<String> {
        let t = app.tree.as_ref().unwrap();
        app.tree_rows
            .iter()
            .map(|r| {
                if r.parent {
                    "..".to_string()
                } else {
                    format!("{}{}", " ".repeat(r.depth()), node_at(t, &r.path).name)
                }
            })
            .collect()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn root_listing_has_parent_row_and_selects_first_entry() {
        let app = app_with(sample());
        assert_eq!(names(&app), ["..", "big", ".hidden", "medium", "small"]);
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn sort_toggles_reverse_and_name() {
        let mut app = app_with(sample());
        app.set_sort(SortKey::Size);
        assert!(app.sort_reverse);
        assert_eq!(names(&app), ["..", "small", "medium", ".hidden", "big"]);
        app.set_sort(SortKey::Name);
        assert!(!app.sort_reverse);
        assert_eq!(names(&app), ["..", ".hidden", "big", "medium", "small"]);
    }

    #[test]
    fn dirs_first_and_hidden_toggle() {
        let mut app = app_with(sample());
        app.dirs_first = true;
        app.show_hidden = false;
        app.rebuild_rows(None);
        assert_eq!(names(&app), ["..", "big", "medium", "small"]);
    }

    #[test]
    fn enter_and_parent_restore_selection() {
        let mut app = app_with(sample());
        assert_eq!(names(&app)[app.cursor], "big");
        app.enter();
        assert_eq!(app.path, vec![1]);
        assert_eq!(names(&app), ["..", "x", "y"]);
        assert_eq!(app.cursor, 1);
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
        assert_eq!(names(&app), ["..", "medium", "small"]);
    }

    #[test]
    fn esc_clears_filter_then_quits() {
        let mut app = app_with(sample());
        app.on_key(key(KeyCode::Char('/')));
        app.on_key(key(KeyCode::Char('m')));
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.filter, "m");
        app.on_key(key(KeyCode::Esc));
        assert!(app.filter.is_empty());
        assert!(!app.should_quit);
        app.on_key(key(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn q_no_longer_quits() {
        let mut app = app_with(sample());
        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit);
    }

    #[test]
    fn ensure_visible_scrolls() {
        let mut app = app_with(sample());
        app.cursor = 4;
        app.ensure_visible(2);
        assert_eq!(app.scroll, 3);
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

    #[test]
    fn fix_path_after_delete_cases() {
        let mut p = vec![3, 1];
        fix_path_after_delete(&mut p, &[1]);
        assert_eq!(p, vec![2, 1]); // earlier sibling removed: shift
        let mut p = vec![3, 1];
        fix_path_after_delete(&mut p, &[5]);
        assert_eq!(p, vec![3, 1]); // later sibling: unchanged
        let mut p = vec![3, 1];
        fix_path_after_delete(&mut p, &[3]);
        assert_eq!(p, Vec::<usize>::new()); // ancestor removed: cut
        let mut p = vec![3, 1];
        fix_path_after_delete(&mut p, &[3, 0]);
        assert_eq!(p, vec![3, 0]); // sibling of the leaf: shift
        let mut p = vec![0];
        fix_path_after_delete(&mut p, &[0, 2]);
        assert_eq!(p, vec![0]); // deletion below the current dir: unchanged
    }

    #[test]
    fn tree_view_flattens_expanded_nodes_only() {
        let mut app = app_with(sample());
        app.switch_view();
        assert_eq!(app.view, View::Tree);
        assert_eq!(
            tree_names(&app),
            ["..", "/root", " big", " .hidden", " medium", " small"]
        );
        // Selection carried over from list view ("big").
        assert_eq!(tree_names(&app)[app.cursor], " big");
        let row = app.selected_tree_row().unwrap();
        assert!(!row.is_last);
        assert!(row.guides.is_empty());

        app.on_key(key(KeyCode::Char('+')));
        assert_eq!(
            tree_names(&app),
            [
                "..", "/root", " big", "  x", "  y", " .hidden", " medium", " small"
            ]
        );
        let y = &app.tree_rows[4];
        assert_eq!(y.guides, vec![false]); // "big" is not last, so a guide line is drawn
        assert!(y.is_last);

        app.on_key(key(KeyCode::Char('-')));
        assert_eq!(tree_names(&app).len(), 6);
        app.on_key(key(KeyCode::Char(' ')));
        assert_eq!(tree_names(&app).len(), 8);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(tree_names(&app).len(), 6);
    }

    #[test]
    fn tree_arrows_expand_step_and_collapse() {
        let mut app = app_with(sample());
        app.switch_view();
        app.on_key(key(KeyCode::Right)); // expand big
        assert!(app.selected_node().unwrap().expanded);
        app.on_key(key(KeyCode::Right)); // step onto x
        assert_eq!(tree_names(&app)[app.cursor], "  x");
        app.on_key(key(KeyCode::Left)); // x is a file: jump to parent
        assert_eq!(tree_names(&app)[app.cursor], " big");
        app.on_key(key(KeyCode::Left)); // collapse big
        assert!(!app.selected_node().unwrap().expanded);
        app.on_key(key(KeyCode::Left)); // jump to root
        assert_eq!(tree_names(&app)[app.cursor], "/root");
    }

    #[test]
    fn tree_expand_all_and_switch_back_to_list() {
        let mut app = app_with(sample());
        app.switch_view();
        app.cursor = 1; // root row
        app.on_key(key(KeyCode::Char('*')));
        assert_eq!(app.tree_rows.len(), 8);
        // Select "y" inside big, switch back: list shows big's listing with y selected.
        app.cursor = app
            .tree_rows
            .iter()
            .position(|r| r.path == vec![1, 1])
            .unwrap();
        app.switch_view();
        assert_eq!(app.view, View::List);
        assert_eq!(app.path, vec![1]);
        assert_eq!(names(&app)[app.cursor], "y");
    }

    #[test]
    fn tree_share_base_is_parent() {
        let mut app = app_with(sample());
        app.switch_view();
        app.on_key(key(KeyCode::Char('+')));
        app.cursor = app
            .tree_rows
            .iter()
            .position(|r| r.path == vec![1, 0])
            .unwrap();
        assert_eq!(app.selected_share_base(), 1000);
        app.cursor = 1;
        assert_eq!(app.selected_share_base(), 1410);
    }

    #[test]
    fn tree_filter_keeps_directories() {
        let mut app = app_with(sample());
        app.switch_view();
        app.filter = "x".into();
        app.rebuild();
        app.cursor = 2;
        app.on_key(key(KeyCode::Char('*')));
        assert_eq!(tree_names(&app), ["..", "/root", " big", "  x"]);
    }
}
