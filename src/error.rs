use thiserror::Error;
use std::path::PathBuf;

#[derive(Debug, Error)]
pub enum SequenceError {
    #[error("IO error: {0}")] 
    Io(#[from]std::io::Error),

    #[error("operation would overwrite {} existing file(s)", .0.len())]
    Conflict(Vec<PathBuf>),

    #[error("suffix cannot contain digits {0}")]
    DigitsInSuffix(String),

    #[error("no sequence found matching {0}")]
    NoSequence(String),

    #[error("offset {0} would yield negative frame numbers")]
    NegativeFrame(isize),

    #[error("cannot create sequence with no items")]
    EmptySequence,
}