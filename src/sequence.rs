use crate::{
    components::Components,
    error::SequenceError,
    item::Item,
    operation::{OperationPlan, Planned},
};
use std::{
    collections::{BTreeMap, HashSet},
    ops::Index,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct FileSequence {
    items: Vec<Item>,
}

/// The outcome of parsing a set of filenames: the sequences that were found,
/// plus any filenames that could not be parsed as sequence items ("rogues").
#[derive(Debug, Clone, Default)]
pub struct ParseResult {
    /// Sequences discovered, ordered deterministically by grouping key.
    pub sequences: Vec<FileSequence>,
    /// Paths of files that did not parse (no frame number). Dotfiles are not
    /// included here — they are skipped silently.
    pub rogues: Vec<PathBuf>,
}

impl FileSequence {
    pub fn new(items: Vec<Item>) -> Result<Self, SequenceError> {
        if items.is_empty() {
            Err(SequenceError::EmptySequence)
        } else {
            Ok(FileSequence { items })
        }
    }

    /// Parses a list of bare filenames into sequences, also reporting any
    /// names that could not be parsed.
    ///
    /// Filenames are grouped by `(prefix, delimiter, suffix, extension)`; each
    /// group with at least `min_frames` items becomes one [`FileSequence`],
    /// with its items sorted by frame number. Dotfiles are skipped silently;
    /// non-dotfiles that contain no frame number are returned in
    /// [`ParseResult::rogues`]. Sequences are ordered deterministically by
    /// their grouping key.
    ///
    /// When a group contains the same frame number at more than one padding, it
    /// is split into a main sequence (using the most common padding) plus one
    /// anomalous sequence per off-nominal padding. Only the split fragments
    /// that still have at least two frames are kept.
    pub fn parse_filenames(
        filenames: &[impl AsRef<str>],
        min_frames: usize,
        directory: Option<PathBuf>,
    ) -> ParseResult {
        let mut groups: BTreeMap<(String, String, String, String), Vec<Item>> = BTreeMap::new();
        let mut rogues: Vec<PathBuf> = Vec::new();

        for name in filenames {
            let name = name.as_ref();
            if name.starts_with('.') {
                continue;
            }
            let Some(item) = Item::from_filename(name, directory.clone()) else {
                rogues.push(match &directory {
                    Some(dir) => dir.join(name),
                    None => PathBuf::from(name),
                });
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

        let mut sequences = Vec::new();
        for mut items in groups.into_values() {
            if items.len() < min_frames {
                continue;
            }
            items.sort_by_key(|i| i.frame_number());
            let seq = FileSequence { items };
            if seq.find_duplicate_frames().is_empty() {
                sequences.push(seq);
            } else {
                sequences.extend(seq.split_on_duplicates());
            }
        }

        ParseResult { sequences, rogues }
    }

    /// Parses a list of bare filenames and returns just the sequences.
    ///
    /// Convenience wrapper over [`FileSequence::parse_filenames`] that discards
    /// the rogue list. See it for the grouping and splitting rules.
    pub fn from_filenames(
        filenames: &[impl AsRef<str>],
        min_frames: usize,
        directory: Option<PathBuf>,
    ) -> Vec<FileSequence> {
        FileSequence::parse_filenames(filenames, min_frames, directory).sequences
    }

    /// Reads a directory and parses its files into sequences, reporting rogues.
    ///
    /// Only regular files are considered. See [`FileSequence::parse_filenames`]
    /// for the grouping rules.
    pub fn parse_directory(
        directory: &Path,
        min_frames: usize,
    ) -> Result<ParseResult, SequenceError> {
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
        Ok(FileSequence::parse_filenames(
            &filenames,
            min_frames,
            Some(directory.to_path_buf()),
        ))
    }

    /// Reads a directory and returns just the sequences it contains.
    ///
    /// Convenience wrapper over [`FileSequence::parse_directory`] that discards
    /// the rogue list.
    pub fn from_directory(
        directory: &Path,
        min_frames: usize,
    ) -> Result<Vec<FileSequence>, SequenceError> {
        Ok(FileSequence::parse_directory(directory, min_frames)?.sequences)
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
    pub fn suffix(&self) -> Result<Option<&str>, SequenceError> {
        let first = self.items.first().unwrap().suffix();
        if self.items.iter().any(|i| i.suffix() != first) {
            return Err(SequenceError::InconsistentProperty("suffix"));
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

    /// Returns the most common padding among the items.
    ///
    /// Unlike [`FileSequence::padding`], this never errors on inconsistent
    /// padding — it picks the dominant width, breaking ties toward the smaller
    /// padding. Used internally when reasoning about anomalous frames.
    pub fn nominal_padding(&self) -> usize {
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for item in &self.items {
            *counts.entry(item.padding()).or_insert(0) += 1;
        }
        // BTreeMap iterates by ascending padding, so `max_by_key` on count
        // keeps the first (smallest padding) seen on ties.
        counts
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(padding, _)| padding)
            .unwrap_or(0)
    }

    /// Renders the sequence as a hash-notation pattern, e.g. `render_####.exr`.
    ///
    /// The hash count is the [`nominal_padding`]. Errors if the prefix,
    /// delimiter, suffix, or extension are inconsistent across items.
    ///
    /// [`nominal_padding`]: FileSequence::nominal_padding
    pub fn sequence_string(&self) -> Result<String, SequenceError> {
        let hashes = "#".repeat(self.nominal_padding());
        Ok(format!(
            "{}{}{}{}.{}",
            self.prefix()?,
            self.delimiter()?.unwrap_or(""),
            hashes,
            self.suffix()?.unwrap_or(""),
            self.extension()?,
        ))
    }

    /// Renders the sequence as a printf-notation pattern, e.g. `render_%04d.exr`.
    ///
    /// Like [`sequence_string`] but using `%0Nd` for the frame placeholder.
    ///
    /// [`sequence_string`]: FileSequence::sequence_string
    pub fn sequence_string_printf(&self) -> Result<String, SequenceError> {
        let token = format!("%0{}d", self.nominal_padding());
        Ok(format!(
            "{}{}{}{}.{}",
            self.prefix()?,
            self.delimiter()?.unwrap_or(""),
            token,
            self.suffix()?.unwrap_or(""),
            self.extension()?,
        ))
    }

    /// Returns the number of items in the sequence (frames actually present).
    pub fn actual_frame_count(&self) -> usize {
        self.items.len()
    }

    /// Returns the span of the sequence: `last_frame - first_frame + 1`,
    /// counting missing frames in between.
    pub fn frame_count(&self) -> i32 {
        self.last_frame() - self.first_frame() + 1
    }

    /// Iterates over the items in frame order.
    pub fn iter(&self) -> std::slice::Iter<'_, Item> {
        self.items.iter()
    }

    /// Returns `true` if the given frame number is present in the sequence.
    pub fn contains(&self, frame: i32) -> bool {
        self.items.iter().any(|i| i.frame_number() == frame)
    }

    /// Returns the item with the given frame number, if present.
    pub fn get_frame(&self, frame: i32) -> Option<&Item> {
        self.items.iter().find(|i| i.frame_number() == frame)
    }

    /// Returns a new sequence containing only the frames in the inclusive
    /// range `start..=end`. The result may be empty.
    pub fn frames(&self, start: i32, end: i32) -> FileSequence {
        let mut items: Vec<Item> = self
            .items
            .iter()
            .filter(|i| (start..=end).contains(&i.frame_number()))
            .cloned()
            .collect();
        items.sort_by_key(|i| i.frame_number());
        FileSequence { items }
    }

    /// Identifies frames that appear more than once (typically at differing
    /// padding). Each entry maps a frame number to all items at that frame,
    /// ordered so the item matching the sequence's [`nominal_padding`] comes
    /// first.
    ///
    /// [`nominal_padding`]: FileSequence::nominal_padding
    pub fn find_duplicate_frames(&self) -> BTreeMap<i32, Vec<Item>> {
        let mut groups: BTreeMap<i32, Vec<Item>> = BTreeMap::new();
        for item in &self.items {
            groups.entry(item.frame_number()).or_default().push(item.clone());
        }

        let nominal = self.nominal_padding();
        groups
            .into_iter()
            .filter(|(_, items)| items.len() > 1)
            .map(|(frame, mut items)| {
                items.sort_by_key(|i| (i.padding() != nominal, i.padding(), i.filename()));
                (frame, items)
            })
            .collect()
    }

    /// Splits a sequence that contains duplicate frame numbers into a main
    /// sequence (items at the most common padding) plus one anomalous sequence
    /// per off-nominal padding. Only fragments with at least two frames are
    /// returned.
    fn split_on_duplicates(&self) -> Vec<FileSequence> {
        let duplicates = self.find_duplicate_frames();
        let nominal = self.nominal_padding();

        let mut main_items: Vec<Item> = Vec::new();
        let mut anomalous: BTreeMap<usize, Vec<Item>> = BTreeMap::new();
        let mut processed: HashSet<i32> = HashSet::new();

        for item in &self.items {
            let frame = item.frame_number();
            if !processed.insert(frame) {
                continue;
            }
            match duplicates.get(&frame) {
                Some(dups) => {
                    for dup in dups {
                        if dup.padding() == nominal {
                            main_items.push(dup.clone());
                        } else {
                            anomalous.entry(dup.padding()).or_default().push(dup.clone());
                        }
                    }
                }
                None => main_items.push(item.clone()),
            }
        }

        let mut result = Vec::new();
        if main_items.len() >= 2 {
            main_items.sort_by_key(|i| i.frame_number());
            result.push(FileSequence { items: main_items });
        }
        for (_, mut items) in anomalous {
            if items.len() >= 2 {
                items.sort_by_key(|i| i.frame_number());
                result.push(FileSequence { items });
            }
        }
        result
    }
}

impl Index<usize> for FileSequence {
    type Output = Item;

    /// Positional access into the frame-ordered items (0-based), like a slice.
    /// For lookup by frame number, use [`FileSequence::get_frame`].
    fn index(&self, index: usize) -> &Item {
        &self.items[index]
    }
}

impl<'a> IntoIterator for &'a FileSequence {
    type Item = &'a Item;
    type IntoIter = std::slice::Iter<'a, Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
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

    /// Prepares a plan to copy every item in the sequence.
    ///
    /// `new_name` optionally rewrites filename components; `new_directory`
    /// optionally targets a different directory. Both are applied per item.
    pub fn copy(
        &self,
        new_name: Option<Components>,
        new_directory: Option<PathBuf>,
    ) -> Planned<FileSequence> {
        let mut plan = OperationPlan::new();
        let mut new_items = Vec::new();
        for item in &self.items {
            let result = item.copy_to(new_name.clone(), new_directory.clone());
            new_items.push(result.proposed);
            plan.extend(result.plan);
        }
        Planned {
            proposed: FileSequence { items: new_items },
            plan,
        }
    }

    /// Prepares a plan to shift every frame number by `offset`.
    ///
    /// `padding` optionally sets the new zero-padding width; it is widened if
    /// necessary so the highest resulting frame still fits. Renames are emitted
    /// high-frame-first when shifting up (and low-first when shifting down) to
    /// avoid clobbering not-yet-moved frames. A zero offset is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`SequenceError::NegativeFrame`] if the shift would produce a
    /// negative frame number.
    pub fn offset_frames(
        &self,
        offset: i32,
        padding: Option<usize>,
    ) -> Result<Planned<FileSequence>, SequenceError> {
        if offset == 0 {
            return Ok(Planned {
                proposed: self.clone(),
                plan: OperationPlan::new(),
            });
        }
        if self.first_frame() + offset < 0 {
            return Err(SequenceError::NegativeFrame(offset as isize));
        }

        let mut padding = padding.unwrap_or_else(|| self.nominal_padding());
        padding = padding.max((self.last_frame() + offset).to_string().len());

        let mut ordered: Vec<&Item> = self.items.iter().collect();
        ordered.sort_by_key(|i| i.frame_number());
        if offset > 0 {
            ordered.reverse();
        }

        let mut plan = OperationPlan::new();
        let mut new_items = Vec::new();
        for item in ordered {
            let result = item.with_frame_number(item.frame_number() + offset, Some(padding));
            new_items.push(result.proposed);
            plan.extend(result.plan);
        }
        new_items.sort_by_key(|i| i.frame_number());

        Ok(Planned {
            proposed: FileSequence { items: new_items },
            plan,
        })
    }

    /// Prepares a plan to re-pad every frame to `padding` digits.
    ///
    /// The width is widened if needed so the highest frame still fits.
    pub fn with_padding(&self, padding: usize) -> Planned<FileSequence> {
        let padding = padding.max(self.last_frame().to_string().len());
        let mut plan = OperationPlan::new();
        let mut new_items = Vec::new();
        for item in &self.items {
            let result = item.with_padding(padding);
            new_items.push(result.proposed);
            plan.extend(result.plan);
        }
        Planned {
            proposed: FileSequence { items: new_items },
            plan,
        }
    }
}
