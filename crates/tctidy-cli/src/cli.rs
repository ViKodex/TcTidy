//! The command-line surface (clap derive).

use clap::Parser;

/// Format TwinCAT 3 Structured Text.
#[derive(Parser, Debug)]
#[command(name = "tctidy", version, about)]
pub struct Cli {
    /// Internal installer protocol: list supported XAE Shell instances.
    #[arg(
        long,
        hide = true,
        conflicts_with_all = [
            "clear_xae_cache",
            "print_config",
            "settings",
            "paths",
            "files_from",
            "check",
            "diff",
            "decl",
            "impl_"
        ]
    )]
    pub discover_xae_shells: bool,

    /// Internal installer protocol: clear one XAE Shell cache for this user.
    #[arg(
        long,
        hide = true,
        value_name = "VERSION",
        value_parser = ["15.0", "17.0"],
        conflicts_with_all = [
            "discover_xae_shells",
            "print_config",
            "settings",
            "paths",
            "files_from",
            "check",
            "diff",
            "decl",
            "impl_"
        ]
    )]
    pub clear_xae_cache: Option<String>,

    /// Files, directories, or globs to format. Use `-` to read ST from stdin.
    pub paths: Vec<String>,

    /// Read target paths (one per line) from this file. Use `-` for stdin.
    /// Blank lines and lines starting with `#` are ignored.
    #[arg(long = "files-from", value_name = "FILE")]
    pub files_from: Option<String>,

    /// Do not write anything; exit non-zero if any file is not already formatted.
    #[arg(long)]
    pub check: bool,

    /// Print a unified diff of what would change, without writing.
    #[arg(long)]
    pub diff: bool,

    /// Use this configuration file instead of discovering the nearest
    /// `.tctidy.toml` for each input.
    #[arg(long, value_name = "FILE")]
    pub config: Option<String>,

    /// Print the effective configuration (every setting, as `.tctidy.toml`)
    /// and exit. Uses `--config`, or the `.tctidy.toml` found from
    /// `--stdin-filepath` or the current directory, or the defaults.
    #[arg(
        long,
        conflicts_with_all = ["paths", "files_from", "check", "diff", "decl", "impl_"]
    )]
    pub print_config: bool,

    /// For stdin input, the path of the file the text comes from (it need not
    /// exist). The `.tctidy.toml` that applies to that file is used, instead of
    /// the one found from the current directory.
    #[arg(long, value_name = "PATH", conflicts_with = "files_from")]
    pub stdin_filepath: Option<String>,

    /// Serve a settings page, with a live preview of each setting, for the
    /// `.tctidy.toml` that applies to DIR (default: the current directory):
    /// the nearest one in DIR or a parent, else a new one in DIR. Prints the
    /// page's local URL, then serves it until the page has been closed for 10
    /// minutes.
    #[arg(
        long,
        value_name = "DIR",
        num_args = 0..=1,
        default_missing_value = ".",
        conflicts_with_all = [
            "paths",
            "files_from",
            "check",
            "diff",
            "decl",
            "impl_",
            "print_config",
            "config",
            "stdin_filepath"
        ]
    )]
    pub settings: Option<String>,

    /// For raw ST input (stdin or a non-XML file), treat it as a
    /// `<Declaration>` section.
    #[arg(long, conflicts_with = "impl_")]
    pub decl: bool,

    /// For raw ST input (stdin or a non-XML file), treat it as an
    /// `<Implementation>` body (the default).
    #[arg(long = "impl")]
    pub impl_: bool,
}
