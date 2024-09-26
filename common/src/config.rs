//! The config module provides configuration-loading mechanism for OpenRank programs.

use serde::de::DeserializeOwned;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(thiserror::Error, Debug)]
pub enum LoadError {
    #[error("user directories for {0:?} cannot be determined")]
    NoUserDirs(String),
    #[error("cannot read config file: {0}")]
    CannotRead(std::io::Error),
    #[error("cannot create config file with defaults: {0}")]
    CannotCreateDefault(std::io::Error),
    #[error("cannot parse TOML config: {0}")]
    CannotParseToml(toml::de::Error),
}

/// OpenRank program configuration loader.
///
/// ```no_run
/// // uses ~/.config/openrank-computer dir
/// let loader = openrank_common::config::Loader::new("openrank-computer")?;
///
/// // loads ~/.config/openrank-computer/openrank-computer.toml
/// let config = loader.load()?;
///
/// // loads ~/.config/openrank-computer/x.toml
/// let config = loader.load_named("x")?;
/// ```
pub struct Loader {
    program_name: String,
    config_dir: PathBuf,
}

impl Loader {
    /// Creates a loader for the given program.
    /// Uses the XDG user directory layout,
    /// e.g. `~/.config/<program-name>`, `~/Library/Application Support/com.openrank.<program-name>`
    /// or `C:\Users\name\AppData\Roaming\openrank\<program-name>\config`
    pub fn new(program_name: &str) -> Result<Self, LoadError> {
        use LoadError::*;
        let dirs = directories::ProjectDirs::from("com", "openrank", program_name)
            .ok_or(NoUserDirs(program_name.into()))?;
        Self::new_with_dir(program_name, dirs.config_dir())
    }

    /// Creates a loader with a specific config directory.
    pub fn new_with_dir(program_name: &str, config_dir: &Path) -> Result<Self, LoadError> {
        Ok(Self { program_name: program_name.to_string(), config_dir: config_dir.into() })
    }

    /// Loads the main TOML config file, named after the program itself,
    /// e.g. `Loader::new("openrank-computer")?.load()?` would load
    /// `~/.config/openrank-computer/openrank-computer.toml`
    pub fn load<T: DeserializeOwned>(&self) -> Result<T, LoadError> {
        self.load_named(&self.program_name)
    }

    /// Loads a TOML config file with the given name,
    /// e.g. `Loader::new("openrank-computer")?.load_named("secrets")?` would load
    /// `~/.config/openrank-computer/secrets.toml`
    pub fn load_named<T: DeserializeOwned>(&self, name: &str) -> Result<T, LoadError> {
        use LoadError::*;
        let path = self.config_dir.join(PathBuf::from(name).with_extension("toml"));
        let content = std::fs::read_to_string(path).map_err(CannotRead)?;
        toml::from_str::<T>(&content).map_err(CannotParseToml)
    }

    /// Loads a TOML config file; if not found, creates a new one with the given default config.
    pub fn load_or_create_named<T: DeserializeOwned>(
        &self, name: &str, default: &str,
    ) -> Result<T, LoadError> {
        use LoadError::*;
        let path = self.config_dir.join(PathBuf::from(name).with_extension("toml"));
        let content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                info!(
                    path = path.to_str(),
                    "creating config with default contents"
                );
                std::fs::create_dir_all(&self.config_dir).map_err(CannotCreateDefault)?;
                std::fs::write(&path, default).map_err(CannotCreateDefault)?;
                default.into()
            },
            Err(err) => return Err(CannotRead(err)),
        };
        toml::from_str::<T>(&content).map_err(CannotParseToml)
    }

    /// Loads the main TOML config file; if not found, creates a new one with the given default config.
    pub fn load_or_create<T: DeserializeOwned>(&self, default: &str) -> Result<T, LoadError> {
        self.load_or_create_named(&self.program_name, default)
    }
}
