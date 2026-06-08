use crate::error::SequenceError;
use std::fs;
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

    /// Performs this operation on the filesystem.
    ///
    /// `Rename` and `Move` both use [`std::fs::rename`], which does not copy
    /// across filesystems; a cross-device move will surface as an error rather
    /// than panicking (a copy+remove fallback is intentionally left for later).
    pub fn execute(&self) -> std::io::Result<()> {
        match self {
            Self::Rename { source, destination } | Self::Move { source, destination } => {
                fs::rename(source, destination)
            }
            Self::Copy { source, destination } => fs::copy(source, destination).map(|_| ()),
            Self::Delete { source } => fs::remove_file(source),
        }
    }
}

/// The outcome of executing an [`OperationPlan`].
#[derive(Debug)]
pub struct ExecutionResult {
    /// Operations that completed successfully, in execution order.
    pub executed: Vec<FileOperation>,
    /// Operations that failed, paired with the error that stopped them.
    pub failed: Vec<(FileOperation, std::io::Error)>,
}

impl ExecutionResult {
    /// Returns `true` if every operation succeeded.
    pub fn success(&self) -> bool {
        self.failed.is_empty()
    }

    /// Returns the number of operations that executed successfully.
    pub fn count(&self) -> usize {
        self.executed.len()
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

    /// The operations in this plan, in order.
    pub fn operations(&self) -> &[FileOperation] {
        &self.operations
    }

    /// Number of operations in the plan.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// Returns `true` if the plan contains no operations.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Executes every operation in the plan.
    ///
    /// If any operation would overwrite an existing file and `force` is
    /// `false`, nothing runs and [`SequenceError::Conflict`] is returned with
    /// the offending destinations. Otherwise each operation is attempted and
    /// the per-operation outcomes are collected into an [`ExecutionResult`];
    /// a single failure does not abort the remaining operations.
    pub fn execute(&self, force: bool) -> Result<ExecutionResult, SequenceError> {
        if !force {
            let conflicts: Vec<PathBuf> = self
                .conflicts()
                .iter()
                .filter_map(|op| op.destination().map(Path::to_path_buf))
                .collect();
            if !conflicts.is_empty() {
                return Err(SequenceError::Conflict(conflicts));
            }
        }

        let mut executed = Vec::new();
        let mut failed = Vec::new();
        for op in &self.operations {
            match op.execute() {
                Ok(()) => executed.push(op.clone()),
                Err(e) => failed.push((op.clone(), e)),
            }
        }

        Ok(ExecutionResult { executed, failed })
    }
}

impl Default for OperationPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// A proposed new state (`proposed`) paired with the [`OperationPlan`] that
/// would bring it about. The plan can be inspected before being run.
pub struct Planned<T> {
    pub proposed: T,
    pub plan: OperationPlan,
}

impl<T> Planned<T> {
    /// Executes the plan and, on success, returns the proposed new state.
    ///
    /// See [`OperationPlan::execute`] for the meaning of `force`. If any
    /// operation fails during execution, the first error is returned.
    pub fn apply(self, force: bool) -> Result<T, SequenceError> {
        let result = self.plan.execute(force)?;
        if let Some((_, err)) = result.failed.into_iter().next() {
            return Err(SequenceError::Io(err));
        }
        Ok(self.proposed)
    }
}
