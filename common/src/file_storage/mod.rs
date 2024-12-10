use std::{error::Error as StdError, fmt::{Display, Formatter, Result as FmtResult}};

use getset::Getters;
use serde::{Serialize, Deserialize};

use crate::tx::Tx;

#[derive(Debug)]
/// Errors that can arise while using database.
pub enum Error {
    /// Error when decoding entries from file.
    Serde(serde_json::Error),
    /// Error when entry is not found.
    NotFound,
    /// Error in config.
    Config(String),
}

impl StdError for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Serde(e) => write!(f, "{}", e),
            Self::NotFound => write!(f, "NotFound"),
            Self::Config(msg) => write!(f, "Config({:?})", msg),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Getters)]
#[getset(get = "pub")]
pub struct Config {
    directory: String,
}

#[derive(Debug)]
pub struct FileStorage {
    buffers: Vec<Tx>,
    last_tx_index: usize,
    config: Config,
}

impl FileStorage {
    pub fn new(config: Config) -> Self {
        Self {
            buffers: Vec::new(),
            last_tx_index: 0,
            config,
        }
    }

    pub fn put(&mut self, tx: Tx) {
        self.buffers.push(tx);
    }

    pub fn save_to_file(&mut self) -> Result<(), Error> {
        // return if buffer is empty
        if self.buffers.is_empty() {
            return Ok(());
        }

        // prepare file name
        let curr_batch_end = self.last_tx_index + self.buffers.len() - 1;
        let prev_batch_end = self.last_tx_index;

        let file_name = format!("{}_{}.txt", prev_batch_end + 1, curr_batch_end);

        // create file & write buffers to it
        let file_path = format!("{}/{}", self.config.directory, file_name);
        let file = std::fs::File::create(file_path).map_err(|e| Error::Config(e.to_string()))?;
        serde_json::to_writer(file, &self.buffers).map_err(Error::Serde)?;

        // empty the buffer
        self.buffers.clear();

        // save "last_tx_index" 
        self.last_tx_index = curr_batch_end;

        Ok(())
    }
}
