//! Which `.tctidy.toml` applies to each input.
//!
//! `--config FILE` pins one configuration for every input. Otherwise each file
//! uses the nearest `.tctidy.toml` in its directory or an ancestor, and stdin
//! uses the one that applies to `--stdin-filepath`, or else the one found from
//! the current directory; with none found, the house style applies. Each configuration file is loaded and validated once, and an
//! invalid one is reported once, however many files it covers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tctidy::{find_config, load_config, FormatOptions};

/// Resolves and caches the [`FormatOptions`] for each input.
pub struct ConfigResolver {
    explicit: Option<Rc<FormatOptions>>,
    /// Discovered config file (or `None` for the house style) per directory.
    by_dir: HashMap<PathBuf, Option<PathBuf>>,
    /// Loaded options per config file; `Err` once reported as invalid.
    by_file: HashMap<PathBuf, Result<Rc<FormatOptions>, ()>>,
    default: Rc<FormatOptions>,
}

impl ConfigResolver {
    /// A resolver honoring `--config`. Fails (after reporting) when the explicit
    /// file cannot be loaded.
    pub fn new(explicit: Option<&str>) -> Result<Self, ()> {
        let explicit = match explicit {
            Some(path) => Some(Rc::new(load(Path::new(path))?)),
            None => None,
        };
        Ok(ConfigResolver {
            explicit,
            by_dir: HashMap::new(),
            by_file: HashMap::new(),
            default: Rc::new(FormatOptions::default()),
        })
    }

    /// The options for a file at `path`.
    pub fn for_file(&mut self, path: &Path) -> Result<Rc<FormatOptions>, ()> {
        let dir = std::path::absolute(path)
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        self.for_dir(&dir)
    }

    /// The options for stdin: those of the file at `filepath` (which need not
    /// exist) when given, else discovered from the current directory.
    pub fn for_stdin(&mut self, filepath: Option<&Path>) -> Result<Rc<FormatOptions>, ()> {
        match filepath {
            Some(path) => self.for_file(path),
            None => {
                let cwd = std::env::current_dir().unwrap_or_default();
                self.for_dir(&cwd)
            }
        }
    }

    fn for_dir(&mut self, dir: &Path) -> Result<Rc<FormatOptions>, ()> {
        if let Some(opts) = &self.explicit {
            return Ok(Rc::clone(opts));
        }
        let found = self
            .by_dir
            .entry(dir.to_path_buf())
            .or_insert_with(|| find_config(dir))
            .clone();
        match found {
            None => Ok(Rc::clone(&self.default)),
            Some(file) => self
                .by_file
                .entry(file.clone())
                .or_insert_with(|| load(&file).map(Rc::new))
                .clone(),
        }
    }
}

fn load(path: &Path) -> Result<FormatOptions, ()> {
    load_config(path)
        .map(|style| FormatOptions { style })
        .map_err(|e| {
            for line in e.to_string().lines() {
                eprintln!("tctidy: {line}");
            }
        })
}
