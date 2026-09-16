//! Parallel filesystem scanner producing an in-memory tree of [`Node`]s.
//!
//! Directories are walked with rayon's work-stealing pool: each directory's
//! files are processed inline and its subdirectories are scanned in parallel.
//! Progress is published through atomics so the UI can animate while the
//! scan runs, and the scan can be cancelled cooperatively.

use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use globset::GlobSet;
use rayon::prelude::*;

/// What kind of filesystem object a node represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Symlink,
    /// Sockets, FIFOs, devices, or entries whose metadata could not be read.
    Other,
}

/// Bit flags mirroring ncdu's one-character entry markers.
pub mod flags {
    /// Directory could not be read.
    pub const ERR: u8 = 1 << 0;
    /// A subdirectory could not be read, so the size is a lower bound.
    pub const SUB_ERR: u8 = 1 << 1;
    /// Hard link to a file that was already counted elsewhere.
    pub const HARDLINK: u8 = 1 << 2;
    /// Directory contains nothing.
    pub const EMPTY: u8 = 1 << 3;
    /// Directory lives on another filesystem and was not descended into.
    pub const OTHER_FS: u8 = 1 << 4;
    /// Entry matched an exclude pattern and was not counted.
    pub const EXCLUDED: u8 = 1 << 5;
}

#[derive(Debug)]
pub struct Node {
    /// File name (the root node holds the full path instead).
    pub name: Box<str>,
    pub kind: Kind,
    /// Disk usage in bytes (allocated blocks × 512 on Unix).
    pub size: u64,
    /// Apparent size in bytes (`st_size`).
    pub apparent: u64,
    /// Modification time as unix seconds. For directories this is the latest
    /// mtime found anywhere in the subtree.
    pub mtime: i64,
    /// Number of descendants (files + directories), 0 for leaves.
    pub items: u64,
    pub flags: u8,
    /// Tree-view state: whether this directory's children are shown.
    pub expanded: bool,
    pub children: Vec<Node>,
}

impl Node {
    fn new(name: String, kind: Kind, md: Option<&Metadata>) -> Self {
        let (size, apparent, mtime) = match md {
            Some(md) => (disk_size(md), md.len(), mtime_of(md)),
            None => (0, 0, 0),
        };
        Node {
            name: name.into_boxed_str(),
            kind,
            size,
            apparent,
            mtime,
            items: 0,
            flags: 0,
            expanded: false,
            children: Vec::new(),
        }
    }

    /// Expand or collapse this node and every directory beneath it.
    pub fn set_expanded_recursive(&mut self, expanded: bool) {
        if self.is_dir() {
            self.expanded = expanded;
        }
        for c in &mut self.children {
            c.set_expanded_recursive(expanded);
        }
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    /// Size in the currently selected accounting mode.
    pub fn size_of(&self, apparent: bool) -> u64 {
        if apparent { self.apparent } else { self.size }
    }

    /// The ncdu-style flag character for this entry, highest priority first.
    pub fn flag_char(&self) -> Option<char> {
        if self.has(flags::ERR) {
            Some('!')
        } else if self.has(flags::SUB_ERR) {
            Some('.')
        } else if self.has(flags::EXCLUDED) {
            Some('<')
        } else if self.has(flags::OTHER_FS) {
            Some('>')
        } else if self.has(flags::HARDLINK) {
            Some('H')
        } else if matches!(self.kind, Kind::Symlink | Kind::Other) {
            Some('@')
        } else if self.has(flags::EMPTY) {
            Some('e')
        } else {
            None
        }
    }

    /// Human description of the entry kind and flags, for the info popup.
    pub fn describe(&self) -> String {
        let mut parts = vec![
            match self.kind {
                Kind::Dir => "directory",
                Kind::File => "regular file",
                Kind::Symlink => "symbolic link",
                Kind::Other => "special file",
            }
            .to_string(),
        ];
        if self.has(flags::ERR) {
            parts.push("unreadable".into());
        }
        if self.has(flags::SUB_ERR) {
            parts.push("contains unreadable directories".into());
        }
        if self.has(flags::EXCLUDED) {
            parts.push("excluded".into());
        }
        if self.has(flags::OTHER_FS) {
            parts.push("other filesystem".into());
        }
        if self.has(flags::HARDLINK) {
            parts.push("hard link (counted elsewhere)".into());
        }
        if self.has(flags::EMPTY) {
            parts.push("empty".into());
        }
        parts.join(", ")
    }

    /// Recompute aggregate fields from this node's own metadata plus its children.
    fn aggregate(&mut self, own_size: u64, own_apparent: u64, own_mtime: i64) {
        let mut size = own_size;
        let mut apparent = own_apparent;
        let mut items = 0;
        let mut mtime = own_mtime;
        let mut sub_err = false;
        for c in &self.children {
            size += c.size;
            apparent += c.apparent;
            items += 1 + c.items;
            mtime = mtime.max(c.mtime);
            sub_err |= c.has(flags::ERR | flags::SUB_ERR);
        }
        self.size = size;
        self.apparent = apparent;
        self.items = items;
        self.mtime = mtime;
        if sub_err {
            self.flags |= flags::SUB_ERR;
        }
        if self.children.is_empty() && !self.has(flags::ERR) {
            self.flags |= flags::EMPTY;
        }
    }
}

/// Options controlling a scan.
#[derive(Debug, Default)]
pub struct ScanOptions {
    pub one_file_system: bool,
    pub excludes: Option<GlobSet>,
}

/// Live counters updated by the scanner.
#[derive(Debug, Default)]
pub struct Progress {
    pub items: AtomicU64,
    pub bytes: AtomicU64,
    pub current: Mutex<String>,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn current_path(&self) -> String {
        self.current.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

struct Ctx<'a> {
    opts: &'a ScanOptions,
    progress: &'a Progress,
    root_dev: u64,
    hardlinks: Mutex<HashSet<(u64, u64)>>,
}

/// Scan `root` (which must be a directory) and return its tree.
/// The root node's name is the full path as given.
pub fn scan(root: &Path, opts: &ScanOptions, progress: &Progress) -> io::Result<Node> {
    let md = fs::metadata(root)?;
    if !md.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a directory", root.display()),
        ));
    }
    let ctx = Ctx {
        opts,
        progress,
        root_dev: dev_of(&md),
        hardlinks: Mutex::new(HashSet::new()),
    };
    let mut node = scan_dir(root, &md, &ctx);
    node.name = sanitize(&root.display().to_string()).into_boxed_str();
    Ok(node)
}

fn scan_dir(path: &Path, md: &Metadata, ctx: &Ctx) -> Node {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let mut node = Node::new(sanitize(&name), Kind::Dir, Some(md));
    let (own_size, own_apparent, own_mtime) = (node.size, node.apparent, node.mtime);

    if ctx.progress.cancelled() {
        node.aggregate(own_size, own_apparent, own_mtime);
        return node;
    }
    if let Ok(mut cur) = ctx.progress.current.lock() {
        *cur = path.display().to_string();
    }

    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            node.flags |= flags::ERR;
            return node;
        }
    };

    let mut dirs = Vec::new();
    for entry in rd {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                node.flags |= flags::ERR;
                continue;
            }
        };
        ctx.progress.items.fetch_add(1, Ordering::Relaxed);
        let raw_name = entry.file_name();
        let name = sanitize(&raw_name.to_string_lossy());
        let child_path = entry.path();

        // `DirEntry::metadata` does not follow symlinks, which is what we want.
        let md = match entry.metadata() {
            Ok(md) => md,
            Err(_) => {
                let mut n = Node::new(name, Kind::Other, None);
                n.flags |= flags::ERR;
                node.children.push(n);
                continue;
            }
        };

        if let Some(set) = &ctx.opts.excludes
            && (set.is_match(&raw_name) || set.is_match(&child_path))
        {
            let kind = if md.is_dir() { Kind::Dir } else { Kind::File };
            let mut n = Node::new(name, kind, None);
            n.flags |= flags::EXCLUDED;
            node.children.push(n);
            continue;
        }

        let ft = md.file_type();
        if ft.is_dir() {
            if ctx.opts.one_file_system && dev_of(&md) != ctx.root_dev {
                let mut n = Node::new(name, Kind::Dir, Some(&md));
                n.flags |= flags::OTHER_FS;
                node.children.push(n);
                continue;
            }
            dirs.push((child_path, md));
        } else if ft.is_symlink() {
            node.children
                .push(Node::new(name, Kind::Symlink, Some(&md)));
        } else if ft.is_file() {
            let mut n = Node::new(name, Kind::File, Some(&md));
            if is_shared_hardlink(&md, ctx) {
                n.flags |= flags::HARDLINK;
                n.size = 0;
                n.apparent = 0;
            } else {
                ctx.progress.bytes.fetch_add(n.size, Ordering::Relaxed);
            }
            node.children.push(n);
        } else {
            node.children.push(Node::new(name, Kind::Other, Some(&md)));
        }
    }

    if !dirs.is_empty() {
        let scanned: Vec<Node> = dirs
            .into_par_iter()
            .map(|(p, md)| scan_dir(&p, &md, ctx))
            .collect();
        node.children.extend(scanned);
    }

    node.aggregate(own_size, own_apparent, own_mtime);
    node
}

/// Replace control characters so file names can never corrupt the display.
fn sanitize(name: &str) -> String {
    if name.chars().any(char::is_control) {
        name.chars()
            .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
            .collect()
    } else {
        name.to_string()
    }
}

#[cfg(unix)]
fn is_shared_hardlink(md: &Metadata, ctx: &Ctx) -> bool {
    use std::os::unix::fs::MetadataExt;
    if md.nlink() <= 1 {
        return false;
    }
    let key = (md.dev(), md.ino());
    match ctx.hardlinks.lock() {
        Ok(mut set) => !set.insert(key),
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_shared_hardlink(_md: &Metadata, _ctx: &Ctx) -> bool {
    false
}

#[cfg(unix)]
pub fn disk_size(md: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.blocks() * 512
}

#[cfg(not(unix))]
pub fn disk_size(md: &Metadata) -> u64 {
    md.len()
}

#[cfg(unix)]
fn dev_of(md: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.dev()
}

#[cfg(not(unix))]
fn dev_of(_md: &Metadata) -> u64 {
    0
}

#[cfg(unix)]
fn mtime_of(md: &Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt;
    md.mtime()
}

#[cfg(not(unix))]
fn mtime_of(md: &Metadata) -> i64 {
    use std::time::UNIX_EPOCH;
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn write(path: &Path, bytes: usize) {
        let mut f = File::create(path).unwrap();
        f.write_all(&vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn scans_tree_and_aggregates() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("a.txt"), 10);
        fs::create_dir(root.join("sub")).unwrap();
        write(&root.join("sub/b.bin"), 20);
        fs::create_dir(root.join("empty")).unwrap();

        let node = scan(root, &ScanOptions::default(), &Progress::default()).unwrap();
        assert_eq!(node.kind, Kind::Dir);
        assert_eq!(node.children.len(), 3);
        assert_eq!(node.items, 4);
        // Files contribute 30 bytes; directories add their own st_size on top.
        assert!(node.apparent >= 30);
        let sub = node.children.iter().find(|c| &*c.name == "sub").unwrap();
        assert_eq!(sub.items, 1);
        assert_eq!(sub.children[0].apparent, 20);
        let empty = node.children.iter().find(|c| &*c.name == "empty").unwrap();
        assert!(empty.has(flags::EMPTY));
        assert_eq!(empty.flag_char(), Some('e'));
    }

    #[test]
    fn apparent_size_sums_files() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("a"), 100);
        fs::create_dir(root.join("d")).unwrap();
        write(&root.join("d/b"), 50);
        let node = scan(root, &ScanOptions::default(), &Progress::default()).unwrap();
        // Directories contribute their own st_size; files must add up exactly.
        let files: u64 = node.apparent
            - node
                .children
                .iter()
                .filter(|c| c.is_dir())
                .map(|c| c.apparent)
                .sum::<u64>();
        let d = node.children.iter().find(|c| &*c.name == "d").unwrap();
        let own_root = files - 100;
        let d_own = d.apparent - 50;
        assert_eq!(node.apparent, own_root + 100 + d_own + 50);
    }

    #[test]
    fn excludes_are_flagged_and_not_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("keep.txt"), 10);
        write(&root.join("skip.tmp"), 1000);
        let mut b = globset::GlobSetBuilder::new();
        b.add(globset::Glob::new("*.tmp").unwrap());
        let opts = ScanOptions {
            one_file_system: false,
            excludes: Some(b.build().unwrap()),
        };
        let node = scan(root, &opts, &Progress::default()).unwrap();
        let skip = node
            .children
            .iter()
            .find(|c| &*c.name == "skip.tmp")
            .unwrap();
        assert!(skip.has(flags::EXCLUDED));
        assert_eq!(skip.apparent, 0);
        assert_eq!(skip.flag_char(), Some('<'));
    }

    #[cfg(unix)]
    #[test]
    fn hardlinks_counted_once() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("one"), 4096);
        fs::hard_link(root.join("one"), root.join("two")).unwrap();
        let node = scan(root, &ScanOptions::default(), &Progress::default()).unwrap();
        let counted = node.children.iter().filter(|c| c.apparent == 4096).count();
        let linked = node
            .children
            .iter()
            .filter(|c| c.has(flags::HARDLINK))
            .count();
        assert_eq!(counted, 1);
        assert_eq!(linked, 1);
    }

    #[test]
    fn rejects_non_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("file");
        write(&f, 1);
        assert!(scan(&f, &ScanOptions::default(), &Progress::default()).is_err());
    }

    #[test]
    fn sanitizes_control_chars() {
        assert_eq!(sanitize("a\nb"), "a\u{FFFD}b");
        assert_eq!(sanitize("plain"), "plain");
    }
}
