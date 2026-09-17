//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "codu scans a directory tree in parallel and lets you browse it in an \
interactive terminal UI with ncdu-compatible keys, truecolor themes and emoji icons.\n\n\
Configuration: $XDG_CONFIG_HOME/codu/config.toml (see --dump-config)\n\
Themes:        $XDG_CONFIG_HOME/codu/themes/<name>.toml (see --list-themes, --dump-theme)"
)]
pub struct Cli {
    /// Directory to scan (defaults to the current directory)
    pub path: Option<PathBuf>,

    /// Use an alternative config file
    #[arg(short = 'c', long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Theme name (built-in or from the themes directory) or path to a .toml file
    #[arg(short = 'T', long, value_name = "NAME")]
    pub theme: Option<String>,

    /// Icon set to use for the file-type column
    #[arg(long, value_enum, value_name = "MODE")]
    pub icons: Option<IconArg>,

    /// Start in tree view instead of the directory list
    #[arg(long)]
    pub tree: bool,

    /// Stay on the scanned directory's volume (the default)
    #[arg(short = 'x', long, conflicts_with = "cross_volumes")]
    pub one_file_system: bool,

    /// Descend into other mounted volumes instead of listing them as mount points
    #[arg(short = 'X', long)]
    pub cross_volumes: bool,

    /// Start on the volumes screen (every mounted and unmounted volume)
    #[arg(long)]
    pub volumes: bool,

    /// Exclude entries matching this glob (repeatable)
    #[arg(long, value_name = "GLOB")]
    pub exclude: Vec<String>,

    /// Show apparent sizes instead of disk usage
    #[arg(long)]
    pub apparent_size: bool,

    /// Use decimal (SI) units: kB, MB, GB
    #[arg(long)]
    pub si: bool,

    /// Disable deleting and trashing files
    #[arg(short = 'r', long)]
    pub read_only: bool,

    /// Disable mouse support
    #[arg(long)]
    pub no_mouse: bool,

    /// Number of scanner threads (0 = number of CPUs)
    #[arg(short = 't', long, value_name = "N")]
    pub threads: Option<usize>,

    /// List available themes and exit
    #[arg(long)]
    pub list_themes: bool,

    /// Print a built-in theme's TOML to stdout and exit
    #[arg(long, value_name = "NAME")]
    pub dump_theme: Option<String>,

    /// Print the default config.toml to stdout and exit
    #[arg(long)]
    pub dump_config: bool,

    /// On exit, write the directory that was being shown to this file
    /// (used by the shell integration, see --shell)
    #[arg(long, value_name = "FILE")]
    pub cwd_file: Option<std::path::PathBuf>,

    /// Print a shell function that makes `codu` change into the shown directory on exit
    #[arg(long, value_enum, value_name = "SHELL")]
    pub shell: Option<ShellArg>,

    /// Do not change the shell's directory on exit for this run
    #[arg(long)]
    pub no_cd: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ShellArg {
    Bash,
    Zsh,
    Fish,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum IconArg {
    Emoji,
    Ascii,
    None,
}
