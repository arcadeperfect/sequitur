use crate::error::SequenceError;
use std::collections::HashSet;
use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

/// Entry-level existence that does **not** traverse symlinks: a dangling
/// symlink still counts as occupying its path (unlike [`Path::exists`], which
/// follows the link and reports the missing target as absent). Used for every
/// "would this clobber something?" check so a broken symlink is never silently
/// overwritten.
fn path_exists(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok()
}

/// Whether `a` and `b` are the same filesystem entry — distinguishing a
/// case-only/hardlink rename (same inode under a different spelling on a
/// case-insensitive volume) from a rename that would clobber a genuinely
/// different file. On non-Unix targets this conservatively returns `false`,
/// so such a rename is reported as a conflict rather than silently allowed.
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_file(_a: &Path, _b: &Path) -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
        self.destination().is_some_and(path_exists)
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

    /// Whether performing this operation vacates its source path, so a later
    /// operation may safely write there. `Rename`, `Move` and `Delete` free
    /// their source; `Copy` leaves it in place.
    fn frees_source(&self) -> bool {
        matches!(
            self,
            Self::Rename { .. } | Self::Move { .. } | Self::Delete { .. }
        )
    }
}

/// The outcome of executing an [`OperationPlan`].
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExecutionResult {
    /// Operations that completed successfully, in execution order.
    pub executed: Vec<FileOperation>,
    /// Operations that failed, paired with the error that stopped them.
    ///
    /// When serialized, each pair becomes `{ "operation": ..., "error": "..." }`;
    /// the [`std::io::Error`] is rendered to its message string since it is not
    /// itself serializable.
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_failed"))]
    pub failed: Vec<(FileOperation, std::io::Error)>,
}

/// Serializes the `failed` operations, flattening each [`std::io::Error`] to its
/// message string (the error type is not serde-serializable on its own).
#[cfg(feature = "serde")]
fn serialize_failed<S>(
    failed: &[(FileOperation, std::io::Error)],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;

    #[derive(serde::Serialize)]
    struct FailedOp<'a> {
        operation: &'a FileOperation,
        error: String,
    }

    let mut seq = serializer.serialize_seq(Some(failed.len()))?;
    for (operation, error) in failed {
        seq.serialize_element(&FailedOp {
            operation,
            error: error.to_string(),
        })?;
    }
    seq.end()
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

/// A single step's worth of progress, handed to the observer of
/// [`OperationPlan::commit_observed`] just before that step runs.
///
/// `index`/`total` count *scheduled* steps, which include the synthetic temp
/// renames inserted to break cycles, so they can exceed the number of authored
/// operations. The `operation` borrow lets an observer inspect (or display) the
/// step about to execute; a temp hop is recognisable by its `sequitur-tmp`
/// destination.
#[derive(Debug, Clone, Copy)]
pub struct Progress<'a> {
    /// Zero-based index of the step about to run.
    pub index: usize,
    /// Total number of scheduled steps.
    pub total: usize,
    /// The operation about to be executed.
    pub operation: &'a FileOperation,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

    /// Checks the plan against a *projected* path-set — the set of paths that
    /// exist in the world this plan will run against — without touching disk.
    ///
    /// This is the honest, stage-time conflict gate for virtual/staged edits,
    /// where the live filesystem is the wrong reference (a half-built swap like
    /// `A→tmp, B→A` would false-positive on disk, and `A→B, C→A` would
    /// false-negative). A destination conflicts when it is present in `against`
    /// and **no** operation in the plan vacates that path, or when two
    /// operations target the same destination. Returns
    /// [`SequenceError::Conflict`] listing every offending destination.
    ///
    /// Unlike [`conflicts`](OperationPlan::conflicts) /
    /// [`has_conflicts`](OperationPlan::has_conflicts), which test the live
    /// disk per-operation, this reasons about the plan as a whole.
    pub fn validate(&self, against: &HashSet<PathBuf>) -> Result<(), SequenceError> {
        let conflicts = self.collect_conflicts(|d| against.contains(d));
        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(SequenceError::Conflict(conflicts))
        }
    }

    /// Collects every destination that would clobber a path which `exists`
    /// reports as already present and which no operation in the plan frees,
    /// plus any destination targeted by more than one operation. Pure: the
    /// only filesystem knowledge comes from the supplied `exists` predicate.
    fn collect_conflicts(&self, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
        let freed: HashSet<&Path> = self
            .operations
            .iter()
            .filter(|op| op.frees_source())
            .map(|op| op.source())
            .collect();

        let mut seen: HashSet<&Path> = HashSet::new();
        let mut conflicts = Vec::new();
        for op in &self.operations {
            let Some(dest) = op.destination() else {
                continue;
            };
            if !seen.insert(dest) {
                // Two operations cannot both produce the same path.
                conflicts.push(dest.to_path_buf());
            } else if exists(dest) && !freed.contains(dest) {
                conflicts.push(dest.to_path_buf());
            }
        }
        conflicts
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

    /// Orders the operations so every source still exists when its operation
    /// runs, inserting temporary renames to break dependency cycles (e.g. a
    /// two-file swap or an N-way rotation).
    ///
    /// The returned operations are the *concrete* steps to perform, in order;
    /// they may include synthetic `Rename`s into hidden temp paths and out of
    /// them again. Unlike [`OperationPlan::execute`], which runs the plan in
    /// authored order, this never clobbers a path that another operation still
    /// needs as its source.
    ///
    /// Returns [`SequenceError::Conflict`] only if the operations are
    /// genuinely unsatisfiable (a destination that no operation will ever
    /// free and which cannot be hopped through a temp).
    pub fn schedule(&self) -> Result<Vec<FileOperation>, SequenceError> {
        Ok(self.build_schedule()?.0)
    }

    /// Builds the concrete schedule alongside the set of synthetic temp paths
    /// it introduces (needed by [`OperationPlan::commit`] to unwind a partial
    /// cycle on failure).
    fn build_schedule(&self) -> Result<(Vec<FileOperation>, HashSet<PathBuf>), SequenceError> {
        let mut remaining: Vec<FileOperation> = self.operations.clone();

        // Paths that currently exist in the simulated filesystem.
        let mut live: HashSet<PathBuf> =
            remaining.iter().map(|op| op.source().to_path_buf()).collect();
        // Every path the plan mentions, so temp names can avoid all of them.
        let mut known: HashSet<PathBuf> = HashSet::new();
        for op in &remaining {
            known.insert(op.source().to_path_buf());
            if let Some(d) = op.destination() {
                known.insert(d.to_path_buf());
            }
        }

        let mut scheduled: Vec<FileOperation> = Vec::new();
        let mut temps: HashSet<PathBuf> = HashSet::new();
        let mut counter = 0usize;

        while !remaining.is_empty() {
            // An op is runnable when its source exists and its destination is
            // not still occupied by some other op's live source.
            let runnable = remaining.iter().position(|op| {
                live.contains(op.source())
                    && op.destination().is_none_or(|d| !live.contains(d))
            });

            if let Some(idx) = runnable {
                let op = remaining.remove(idx);
                match &op {
                    FileOperation::Copy { destination, .. } => {
                        live.insert(destination.clone());
                    }
                    FileOperation::Rename { source, destination }
                    | FileOperation::Move { source, destination } => {
                        live.remove(source);
                        live.insert(destination.clone());
                    }
                    FileOperation::Delete { source } => {
                        live.remove(source);
                    }
                }
                scheduled.push(op);
                continue;
            }

            // Deadlock: every remaining op is blocked by another's source.
            // Break the cycle by hopping one blocking source through a temp.
            let blockers: HashSet<PathBuf> = remaining
                .iter()
                .filter_map(|op| op.destination().map(Path::to_path_buf))
                .collect();
            let hop = remaining.iter().position(|op| {
                matches!(
                    op,
                    FileOperation::Rename { .. } | FileOperation::Move { .. }
                ) && blockers.contains(op.source())
            });

            match hop {
                Some(idx) => {
                    let source = remaining[idx].source().to_path_buf();
                    let temp = temp_path(&source, &known, &mut counter);
                    known.insert(temp.clone());
                    temps.insert(temp.clone());

                    scheduled.push(FileOperation::Rename {
                        source: source.clone(),
                        destination: temp.clone(),
                    });
                    live.remove(&source);
                    live.insert(temp.clone());

                    match &mut remaining[idx] {
                        FileOperation::Rename { source: s, .. }
                        | FileOperation::Move { source: s, .. } => *s = temp,
                        _ => unreachable!("hop index only selects Rename/Move"),
                    }
                }
                None => {
                    let stuck: Vec<PathBuf> = remaining
                        .iter()
                        .filter_map(|op| op.destination().map(Path::to_path_buf))
                        .collect();
                    return Err(SequenceError::Conflict(stuck));
                }
            }
        }

        Ok((scheduled, temps))
    }

    /// Executes the plan as a cycle-safe transaction.
    ///
    /// Unlike [`OperationPlan::execute`], conflicts are judged against the
    /// *post-plan* filesystem: a destination that already exists is only a
    /// conflict if no operation in the plan frees that path (so a two-file
    /// swap is allowed even though both destinations exist up front). With
    /// `force`, the pre-flight conflict check is skipped entirely.
    ///
    /// Operations are then run in [`schedule`](OperationPlan::schedule) order.
    /// On the first failure, any in-flight temp rename is rolled back so the
    /// filesystem is left without stray `sequitur-tmp` files, and the partial
    /// outcome is returned with the offending operation in `failed`.
    pub fn commit(&self, force: bool) -> Result<ExecutionResult, SequenceError> {
        self.commit_observed(force, |_| ControlFlow::Continue(()))
    }

    /// Like [`OperationPlan::commit`], but reports [`Progress`] to `observe`
    /// before each scheduled step and lets it cancel the run.
    ///
    /// `observe` is called with the step about to execute; returning
    /// [`ControlFlow::Break`] stops the transaction *before* that step runs.
    /// Cancellation is not a failure: already-completed independent operations
    /// stay applied, any in-flight temp cycle is unwound (so no stray
    /// `sequitur-tmp` files remain), and the partial [`ExecutionResult`] is
    /// returned with an empty `failed` list. A genuine I/O error still lands in
    /// `failed` exactly as for [`commit`](OperationPlan::commit).
    pub fn commit_observed<F>(
        &self,
        force: bool,
        mut observe: F,
    ) -> Result<ExecutionResult, SequenceError>
    where
        F: FnMut(Progress) -> ControlFlow<()>,
    {
        if !force {
            let mut conflicts = self.collect_conflicts(path_exists);
            // A case-only or hardlink rename (`render→RENDER` on a
            // case-insensitive filesystem) reports its destination as
            // existing, but renaming there clobbers nothing — it IS the
            // source entry. Drop any "conflict" that is the same file as an
            // operation's own freed source.
            if !conflicts.is_empty() {
                let freed: Vec<&Path> = self
                    .operations
                    .iter()
                    .filter(|op| op.frees_source())
                    .map(|op| op.source())
                    .collect();
                conflicts.retain(|d| !freed.iter().any(|s| same_file(s, d)));
            }
            if !conflicts.is_empty() {
                return Err(SequenceError::Conflict(conflicts));
            }
        }

        let (schedule, temps) = self.build_schedule()?;
        let total = schedule.len();

        let mut executed: Vec<FileOperation> = Vec::new();
        let mut live_temps: Vec<PathBuf> = Vec::new();
        for (index, op) in schedule.into_iter().enumerate() {
            let progress = Progress {
                index,
                total,
                operation: &op,
            };
            if observe(progress).is_break() {
                let failed = drain_temps(&mut executed, &mut live_temps);
                return Ok(ExecutionResult { executed, failed });
            }

            match op.execute() {
                Ok(()) => {
                    if let Some(d) = op.destination() {
                        if temps.contains(d) {
                            live_temps.push(d.to_path_buf());
                        }
                    }
                    if temps.contains(op.source()) {
                        live_temps.retain(|t| t != op.source());
                    }
                    executed.push(op);
                }
                Err(e) => {
                    let mut failed = vec![(op, e)];
                    failed.extend(drain_temps(&mut executed, &mut live_temps));
                    return Ok(ExecutionResult { executed, failed });
                }
            }
        }

        Ok(ExecutionResult {
            executed,
            failed: Vec::new(),
        })
    }
}

/// Picks a hidden temp path next to `source` that collides with neither the
/// plan's known paths nor anything already on disk.
fn temp_path(source: &Path, known: &HashSet<PathBuf>, counter: &mut usize) -> PathBuf {
    let parent = source.parent().unwrap_or_else(|| Path::new("."));
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("seq");
    loop {
        let candidate = parent.join(format!(".{name}.sequitur-tmp-{counter}"));
        *counter += 1;
        if !known.contains(&candidate) && !path_exists(&candidate) {
            return candidate;
        }
    }
}

/// Unwinds an interrupted temp cycle: reverses already-executed renames (in
/// LIFO order) until every live temp path has been moved back, popping each
/// successfully-undone operation out of `executed`.
///
/// Stops at the first operation it cannot or should not reverse — a non-rename
/// step (which was never part of the temp cycle) or a reverse `rename` that
/// itself fails (leaving the file stranded at the temp path). Such an
/// operation is left in `executed` because its on-disk effect persists, and a
/// failed reversal is returned so the caller can report the stranded path
/// rather than claiming a clean rollback.
fn drain_temps(
    executed: &mut Vec<FileOperation>,
    live_temps: &mut Vec<PathBuf>,
) -> Vec<(FileOperation, std::io::Error)> {
    let mut unwind_failed = Vec::new();
    while !live_temps.is_empty() {
        let Some(op) = executed.last() else { break };
        let (FileOperation::Rename { source, destination }
        | FileOperation::Move { source, destination }) = op
        else {
            // Not part of the temp cycle; its effect stands. Stop here.
            break;
        };
        match fs::rename(destination, source) {
            Ok(()) => {
                let dest = destination.clone();
                live_temps.retain(|t| t != &dest);
                executed.pop();
            }
            Err(e) => {
                // The file is stranded at `destination`; leave the op in
                // `executed` (its effect persists) and report the failure.
                unwind_failed.push((op.clone(), e));
                break;
            }
        }
    }
    unwind_failed
}

impl Default for OperationPlan {
    fn default() -> Self {
        Self::new()
    }
}

/// A proposed new state (`proposed`) paired with the [`OperationPlan`] that
/// would bring it about. The plan can be inspected before being run.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
