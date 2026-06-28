//! Crate error type.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("rule `{name}` has an invalid regex: {source}")]
    BadRule {
        name: String,
        #[source]
        source: regex::Error,
    },

    #[error("theme `{0}` not found (try `--list-themes`)")]
    UnknownTheme(String),

    #[error("config file {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
