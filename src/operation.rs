use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOperation {
    Rename {
        source: PathBuf,
        destination: PathBuf,
    },
    Delete {
        source: PathBuf,
    },
    Move {
        source: PathBuf,
        destination: PathBuf,
    },
    Copy {
        source: PathBuf,
        destination: PathBuf,
    },
}

impl FileOperation {
    pub fn source(&self) -> &Path {
        match self {
            Self::Rename { source, .. } => source,
            Self::Delete { source } => source,
            Self::Move { source, .. } => source,
            Self::Copy { source, .. } => source,
        }
    }

    pub fn destination(&self) -> Option<&Path> {
        match self {
            Self::Rename { destination, .. } => Some(destination),
            Self::Delete { .. } => None,
            Self::Move { destination, .. } => Some(destination),
            Self::Copy { destination, .. } => Some(destination),
        }
    }

    pub fn would_overwrite(&self) -> bool {
        self.destination().is_some_and(|d| d.exists())
    }
}

#[derive(Debug, Clone)]
pub struct OperationPlan {
    operations: Vec<FileOperation>,
}

impl OperationPlan {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }
    pub fn conflicts(&self) -> Vec<&FileOperation> {
        self.operations
            .iter()
            .filter(|op| op.would_overwrite())
            .collect()
    }
    pub fn has_conflicts(&self) -> bool {
        self.operations.iter().any(|op| op.would_overwrite())
    }
    pub fn push(&mut self, op: FileOperation) {
        self.operations.push(op);
    }
    pub fn extend(&mut self, ops: OperationPlan) {
        self.operations.extend(ops.operations)
    }
}


pub struct Planned<T> {
    pub proposed: T,
    pub plan: OperationPlan
}
