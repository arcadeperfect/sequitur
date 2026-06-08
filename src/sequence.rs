use crate::{
    components::Components,
    error::SequenceError,
    item::Item,
    operation::{OperationPlan, Planned},
};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct FileSequence {
    items: Vec<Item>,
}

impl FileSequence {
    pub fn new(items: Vec<Item>) -> Result<Self, SequenceError> {
        if items.is_empty() {
            Err(SequenceError::EmptySequence)
        } else {
            Ok(FileSequence { items })
        }
    }

    /// Parses a list of bare filenames and groups them into sequences.
    ///
    /// Filenames are grouped by `(prefix, delimiter, suffix, extension)`; each
    /// group with at least `min_frames` items becomes one [`FileSequence`],
    /// with its items sorted by frame number. Dotfiles and names that don't
    /// parse (no frame number) are skipped. The returned sequences are ordered
    /// deterministically by their grouping key.
    ///
    /// Items with clashing frame numbers but different padding are *not* split
    /// into separate sequences yet (pysequitur's anomalous-sequence handling
    /// is not ported).
    pub fn from_filenames(
        filenames: &[impl AsRef<str>],
        min_frames: usize,
        directory: Option<PathBuf>,
    ) -> Vec<FileSequence> {
        let mut groups: BTreeMap<(String, String, String, String), Vec<Item>> = BTreeMap::new();

        for name in filenames {
            let name = name.as_ref();
            if name.starts_with('.') {
                continue;
            }
            let Some(item) = Item::from_filename(name, directory.clone()) else {
                continue;
            };
            let key = (
                item.prefix().to_string(),
                item.delimiter().unwrap_or("").to_string(),
                item.suffix().unwrap_or("").to_string(),
                item.extension().to_string(),
            );
            groups.entry(key).or_default().push(item);
        }

        groups
            .into_values()
            .filter(|items| items.len() >= min_frames)
            .map(|mut items| {
                items.sort_by_key(|i| i.frame_number());
                FileSequence { items }
            })
            .collect()
    }

    /// Reads a directory and groups its files into sequences.
    ///
    /// Only regular files are considered. See [`FileSequence::from_filenames`]
    /// for the grouping rules.
    pub fn from_directory(
        directory: &Path,
        min_frames: usize,
    ) -> Result<Vec<FileSequence>, SequenceError> {
        let mut filenames: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                filenames.push(name.to_string());
            }
        }
        Ok(FileSequence::from_filenames(
            &filenames,
            min_frames,
            Some(directory.to_path_buf()),
        ))
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the sequence has no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The items in this sequence, ordered by frame number.
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn first_frame(&self) -> i32 {
        self.items.iter().map(|i| i.frame_number()).min().unwrap()
    }
    pub fn last_frame(&self) -> i32 {
        self.items.iter().map(|i| i.frame_number()).max().unwrap()
    }
    pub fn existing_frames(&self) -> Vec<i32> {
        self.items.iter().map(|i| i.frame_number()).collect()
    }
    pub fn missing_frames(&self) -> Vec<i32> {
        let range = self.first_frame()..=self.last_frame();
        let existing_frames: HashSet<i32> = self.existing_frames().into_iter().collect();
        range.filter(|f| !existing_frames.contains(f)).collect()
    }
    pub fn prefix(&self) -> Result<&str, SequenceError> {
        let first = self.items.first().unwrap().prefix();
        if self.items.iter().any(|i| i.prefix() != first) {
            return Err(SequenceError::InconsistentProperty("prefix"));
        }
        Ok(first)
    }
    pub fn extension(&self) -> Result<&str, SequenceError> {
        let first = self.items.first().unwrap().extension();
        if self.items.iter().any(|i| i.extension() != first) {
            return Err(SequenceError::InconsistentProperty("extension"));
        }
        Ok(first)
    }

    pub fn delimiter(&self) -> Result<Option<&str>, SequenceError> {
        let first = self.items.first().unwrap().delimiter();
        if self.items.iter().any(|i| i.delimiter() != first) {
            return Err(SequenceError::InconsistentProperty("delimiter"));
        }
        Ok(first)
    }
    pub fn directory(&self) -> Result<Option<&PathBuf>, SequenceError> {
        let first = self.items.first().unwrap().directory();
        if self.items.iter().any(|i| i.directory() != first) {
            return Err(SequenceError::InconsistentProperty("directory"));
        }
        Ok(first)
    }
    pub fn padding(&self) -> Result<usize, SequenceError> {
        let first = self.items.first().unwrap().padding();
        if self.items.iter().any(|i| i.padding() != first) {
            return Err(SequenceError::InconsistentProperty("padding"));
        }
        Ok(first)
    }
}

impl FileSequence {
    pub fn delete(&self) -> OperationPlan {
        let mut plan = OperationPlan::new();
        for item in &self.items {
            plan.extend(item.delete());
        }

        plan
    }
    pub fn rename(&self, new_name: Components) -> Planned<FileSequence> {
        let mut plan = OperationPlan::new();
        let mut new_items = Vec::new();
        for item in &self.items {
            let new_components = new_name.with_frame_number(item.frame_number());
            let new_item = item.rename(new_components);
            new_items.push(new_item.proposed);
            plan.extend(new_item.plan);
        }
        Planned {
            proposed: FileSequence { items: new_items },
            plan,
        }
    }
    /// Prepares a plan to move every item in the sequence into `directory`.
    pub fn move_to(&self, directory: &Path) -> Planned<FileSequence> {
        let mut plan = OperationPlan::new();
        let mut new_items = Vec::new();
        for item in &self.items {
            let result = item.move_to(None, Some(directory.to_path_buf()));
            new_items.push(result.proposed);
            plan.extend(result.plan);
        }
        Planned {
            proposed: FileSequence { items: new_items },
            plan,
        }
    }
}
