//! File-type icons. Every icon is normalised to exactly two terminal cells so
//! rows always line up, whatever the terminal thinks about emoji widths.

use std::collections::HashMap;

use serde::Deserialize;

use crate::scan::{Kind, Node};
use crate::theme::IconsDef;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IconMode {
    Emoji,
    Ascii,
    None,
}

#[derive(Clone, Debug)]
pub struct IconSet {
    pub mode: IconMode,
    pub app: String,
    pub dir: String,
    pub dir_open: String,
    pub parent: String,
    pub file: String,
    pub symlink: String,
    pub hidden: String,
    pub special: String,
    pub error: String,
    pub empty_dir: String,
    pub spinner: Vec<String>,
    ext: HashMap<String, String>,
    names: HashMap<String, String>,
}

/// Default emoji set. Only code points with East Asian Width = Wide are used,
/// because those are the ones every terminal agrees occupy two cells.
const EMOJI_EXT: &[(&[&str], &str)] = &[
    (
        &[
            "png", "jpg", "jpeg", "gif", "bmp", "webp", "svg", "ico", "tif", "tiff", "heic",
            "avif", "raw", "psd", "xcf",
        ],
        "📷",
    ),
    (
        &[
            "mp3", "flac", "wav", "ogg", "oga", "m4a", "aac", "opus", "wma", "aiff", "mid",
        ],
        "🎵",
    ),
    (
        &[
            "mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "ts",
        ],
        "🎬",
    ),
    (
        &[
            "zip", "tar", "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "lz4", "lzma", "cab",
            "deb", "rpm", "apk", "jar", "whl", "crate",
        ],
        "📦",
    ),
    (&["iso", "img", "qcow2", "vdi", "vmdk", "dmg"], "💿"),
    (&["rs"], "🦀"),
    (&["py", "pyc", "pyi", "ipynb"], "🐍"),
    (&["go"], "🐹"),
    (&["java", "class", "kt", "kts", "scala", "groovy"], "☕"),
    (
        &["c", "h", "cpp", "cc", "cxx", "hpp", "hh", "m", "mm"],
        "🔩",
    ),
    (
        &[
            "sh", "bash", "zsh", "fish", "js", "mjs", "cjs", "ts", "jsx", "tsx", "lua", "rb", "pl",
            "php", "ps1", "bat", "cmd",
        ],
        "📜",
    ),
    (
        &[
            "toml",
            "yaml",
            "yml",
            "json",
            "json5",
            "ini",
            "conf",
            "cfg",
            "env",
            "properties",
            "xml",
            "plist",
        ],
        "🔧",
    ),
    (
        &["md", "markdown", "txt", "rst", "org", "adoc", "tex", "rtf"],
        "📝",
    ),
    (&["pdf", "epub", "mobi", "djvu"], "📕"),
    (&["doc", "docx", "odt", "pages"], "📘"),
    (&["xls", "xlsx", "ods", "csv", "tsv", "numbers"], "📊"),
    (&["ppt", "pptx", "odp", "key"], "📑"),
    (
        &[
            "html", "htm", "css", "scss", "sass", "less", "vue", "svelte", "astro",
        ],
        "🌐",
    ),
    (&["lock"], "🔒"),
    (
        &[
            "pem", "crt", "cer", "key", "pub", "gpg", "asc", "p12", "pfx", "keystore",
        ],
        "🔑",
    ),
    (&["log", "out", "err"], "🧾"),
    (&["ttf", "otf", "woff", "woff2", "eot"], "🔤"),
    (
        &[
            "db", "sqlite", "sqlite3", "sql", "mdb", "accdb", "parquet", "arrow", "feather",
        ],
        "🧮",
    ),
    (
        &[
            "exe", "dll", "so", "dylib", "o", "a", "lib", "bin", "elf", "msi", "appimage", "wasm",
        ],
        "🧩",
    ),
    (
        &[
            "bak",
            "old",
            "orig",
            "swp",
            "swo",
            "tmp",
            "temp",
            "part",
            "crdownload",
        ],
        "🧹",
    ),
    (&["torrent"], "📡"),
    (&["ipa", "xapk", "aab"], "🎮"),
    (&["blend", "obj", "fbx", "stl", "3ds", "gltf", "glb"], "🎨"),
];

const EMOJI_NAMES: &[(&str, &str)] = &[
    ("Cargo.toml", "🦀"),
    ("Cargo.lock", "🦀"),
    ("Dockerfile", "🐳"),
    ("docker-compose.yml", "🐳"),
    ("docker-compose.yaml", "🐳"),
    ("compose.yml", "🐳"),
    ("compose.yaml", "🐳"),
    ("Makefile", "🔧"),
    ("CMakeLists.txt", "🔧"),
    ("LICENSE", "📃"),
    ("LICENSE.md", "📃"),
    ("LICENSE.txt", "📃"),
    ("COPYING", "📃"),
    ("README", "📰"),
    ("README.md", "📰"),
    ("README.txt", "📰"),
    ("CHANGELOG.md", "📰"),
    ("package.json", "📦"),
    ("package-lock.json", "🔒"),
    ("yarn.lock", "🔒"),
    ("pnpm-lock.yaml", "🔒"),
    ("go.mod", "🐹"),
    ("go.sum", "🐹"),
    ("pyproject.toml", "🐍"),
    ("requirements.txt", "🐍"),
    ("node_modules", "📦"),
    ("target", "🎯"),
    ("build", "🧱"),
    ("dist", "📦"),
    ("out", "📦"),
    ("bin", "🧩"),
    ("lib", "📚"),
    ("src", "🧬"),
    ("test", "🧪"),
    ("tests", "🧪"),
    ("docs", "📚"),
    ("doc", "📚"),
    ("Downloads", "📥"),
    ("Pictures", "📷"),
    ("Music", "🎵"),
    ("Videos", "🎬"),
    ("Documents", "📝"),
    ("Desktop", "💻"),
    ("Trash", "🧹"),
    ("tmp", "🧹"),
    ("temp", "🧹"),
    ("cache", "👻"),
    (".cache", "👻"),
    (".git", "🌳"),
    (".github", "🌳"),
    (".gitignore", "🌳"),
    (".gitmodules", "🌳"),
    (".ssh", "🔐"),
    (".gnupg", "🔐"),
    (".config", "🔧"),
    (".local", "🏠"),
    (".steam", "🎮"),
    (".cargo", "🦀"),
    (".rustup", "🦀"),
    (".npm", "📦"),
    (".venv", "🐍"),
    ("venv", "🐍"),
    ("__pycache__", "🐍"),
];

impl IconSet {
    /// Build the icon set for `mode`, applying theme overrides (emoji mode only).
    pub fn build(mode: IconMode, overrides: &IconsDef) -> IconSet {
        let mut set = match mode {
            IconMode::Emoji => IconSet::emoji_set(),
            IconMode::Ascii => IconSet::ascii_set(),
            IconMode::None => IconSet::none_set(),
        };
        if mode == IconMode::Emoji {
            set.apply(overrides);
        }
        set.normalize();
        set
    }

    fn emoji_set() -> IconSet {
        let mut ext = HashMap::new();
        for (exts, icon) in EMOJI_EXT {
            for e in *exts {
                ext.insert((*e).to_string(), (*icon).to_string());
            }
        }
        let names = EMOJI_NAMES
            .iter()
            .map(|(n, i)| ((*n).to_string(), (*i).to_string()))
            .collect();
        IconSet {
            mode: IconMode::Emoji,
            app: "💽".into(),
            dir: "📁".into(),
            dir_open: "📂".into(),
            parent: "📂".into(),
            file: "📄".into(),
            symlink: "🔗".into(),
            hidden: "👻".into(),
            special: "🔌".into(),
            error: "❗".into(),
            empty_dir: "📁".into(),
            spinner: ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ext,
            names,
        }
    }

    fn ascii_set() -> IconSet {
        IconSet {
            mode: IconMode::Ascii,
            app: "".into(),
            dir: "/".into(),
            dir_open: "/".into(),
            parent: "..".into(),
            file: " ".into(),
            symlink: "@".into(),
            hidden: ".".into(),
            special: "%".into(),
            error: "!".into(),
            empty_dir: "/".into(),
            spinner: ["|", "/", "-", "\\"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ext: HashMap::new(),
            names: HashMap::new(),
        }
    }

    fn none_set() -> IconSet {
        let mut set = IconSet::ascii_set();
        set.mode = IconMode::None;
        set
    }

    fn apply(&mut self, o: &IconsDef) {
        macro_rules! take {
            ($($f:ident),*) => { $( if let Some(v) = &o.$f { self.$f = v.clone(); } )* };
        }
        take!(
            app, dir, dir_open, parent, file, symlink, hidden, special, error, empty_dir
        );
        if let Some(s) = &o.spinner
            && !s.is_empty()
        {
            self.spinner = s.clone();
        }
        for (k, v) in &o.ext {
            self.ext
                .insert(k.trim_start_matches('.').to_ascii_lowercase(), v.clone());
        }
        for (k, v) in &o.names {
            self.names.insert(k.clone(), v.clone());
        }
    }

    /// Force every icon to exactly two cells.
    fn normalize(&mut self) {
        if self.mode == IconMode::None {
            return;
        }
        let fallback = if self.mode == IconMode::Emoji {
            "📄"
        } else {
            " "
        };
        for s in [
            &mut self.dir,
            &mut self.dir_open,
            &mut self.parent,
            &mut self.file,
            &mut self.symlink,
            &mut self.hidden,
            &mut self.special,
            &mut self.error,
            &mut self.empty_dir,
        ] {
            *s = normalize(s, fallback);
        }
        for v in self.ext.values_mut() {
            *v = normalize(v, fallback);
        }
        for v in self.names.values_mut() {
            *v = normalize(v, fallback);
        }
        if self.mode == IconMode::Emoji {
            self.app = normalize(&self.app, "💽");
        }
        for f in &mut self.spinner {
            *f = normalize(f, " ");
        }
    }

    /// Width of the icon column including its trailing gap (0 when disabled).
    pub fn column_width(&self) -> u16 {
        if self.mode == IconMode::None { 0 } else { 3 }
    }

    /// True only in emoji mode; decorative emoji outside the icon column use this.
    pub fn emoji(&self) -> bool {
        self.mode == IconMode::Emoji
    }

    /// Icon for a node in a listing.
    pub fn for_node(&self, node: &Node) -> &str {
        if self.mode == IconMode::None {
            return "";
        }
        if let Some(i) = self.names.get(&*node.name) {
            return i;
        }
        let hidden = node.name.starts_with('.');
        match node.kind {
            Kind::Dir => {
                if node.has(crate::scan::flags::ERR) {
                    &self.error
                } else if hidden {
                    &self.hidden
                } else if node.has(crate::scan::flags::EMPTY) {
                    &self.empty_dir
                } else {
                    &self.dir
                }
            }
            Kind::File => {
                if let Some(ext) = extension(&node.name)
                    && let Some(i) = self.ext.get(&ext)
                {
                    return i;
                }
                if hidden { &self.hidden } else { &self.file }
            }
            Kind::Symlink => &self.symlink,
            Kind::Other => {
                if node.has(crate::scan::flags::ERR) {
                    &self.error
                } else {
                    &self.special
                }
            }
        }
    }
}

fn extension(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Pad or replace `icon` so that it measures exactly two cells.
pub fn normalize(icon: &str, fallback: &str) -> String {
    let icon = icon.trim_end_matches('\n');
    match crate::format::width(icon) {
        2 => icon.to_string(),
        1 => format!("{icon} "),
        0 if icon.is_empty() => "  ".to_string(),
        _ => normalize_fallback(fallback),
    }
}

fn normalize_fallback(fallback: &str) -> String {
    match crate::format::width(fallback) {
        2 => fallback.to_string(),
        1 => format!("{fallback} "),
        _ => "  ".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::width;

    fn node(name: &str, kind: Kind, flags: u8) -> Node {
        Node {
            name: name.into(),
            kind,
            size: 0,
            apparent: 0,
            mtime: 0,
            items: 0,
            flags,
            children: vec![],
        }
    }

    #[test]
    fn all_default_icons_are_two_cells() {
        let set = IconSet::build(IconMode::Emoji, &IconsDef::default());
        for s in [
            &set.dir,
            &set.dir_open,
            &set.parent,
            &set.file,
            &set.symlink,
            &set.hidden,
            &set.special,
            &set.error,
            &set.empty_dir,
            &set.app,
        ] {
            assert_eq!(width(s), 2, "{s:?}");
        }
        for (exts, icon) in EMOJI_EXT {
            assert_eq!(width(icon), 2, "ext icon {icon:?} for {exts:?}");
        }
        for (name, icon) in EMOJI_NAMES {
            assert_eq!(width(icon), 2, "name icon {icon:?} for {name}");
        }
        for f in &set.spinner {
            assert_eq!(width(f), 2);
        }
    }

    #[test]
    fn normalize_pads_and_replaces() {
        assert_eq!(normalize("a", "📄"), "a ");
        assert_eq!(normalize("📁", "📄"), "📁");
        assert_eq!(normalize("abc", "📄"), "📄");
        assert_eq!(normalize("", "📄"), "  ");
        assert_eq!(normalize("\u{200b}", "x"), "x ");
    }

    #[test]
    fn resolves_by_name_then_ext_then_kind() {
        let set = IconSet::build(IconMode::Emoji, &IconsDef::default());
        assert_eq!(set.for_node(&node("Cargo.toml", Kind::File, 0)), "🦀");
        assert_eq!(set.for_node(&node("main.RS", Kind::File, 0)), "🦀");
        assert_eq!(set.for_node(&node("photo.jpeg", Kind::File, 0)), "📷");
        assert_eq!(set.for_node(&node("notes", Kind::File, 0)), "📄");
        assert_eq!(set.for_node(&node(".bashrc", Kind::File, 0)), "👻");
        assert_eq!(set.for_node(&node(".git", Kind::Dir, 0)), "🌳");
        assert_eq!(set.for_node(&node("src", Kind::Dir, 0)), "🧬");
        assert_eq!(set.for_node(&node("stuff", Kind::Dir, 0)), "📁");
        assert_eq!(set.for_node(&node("link", Kind::Symlink, 0)), "🔗");
        assert_eq!(
            set.for_node(&node("bad", Kind::Dir, crate::scan::flags::ERR)),
            "❗"
        );
    }

    #[test]
    fn overrides_apply_and_are_normalized() {
        let mut o = IconsDef {
            dir: Some("D".into()),
            ..IconsDef::default()
        };
        o.ext.insert(".Foo".into(), "🔥".into());
        let set = IconSet::build(IconMode::Emoji, &o);
        assert_eq!(set.dir, "D ");
        assert_eq!(set.for_node(&node("x.foo", Kind::File, 0)), "🔥");
    }

    #[test]
    fn none_mode_has_no_column() {
        let set = IconSet::build(IconMode::None, &IconsDef::default());
        assert_eq!(set.column_width(), 0);
        assert_eq!(set.for_node(&node("x.rs", Kind::File, 0)), "");
    }
}
