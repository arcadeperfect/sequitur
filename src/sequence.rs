use crate::{
    components::Components,
    error::SequenceError,
    item::Item,
    operation::{OperationPlan, Planned},
};
use std::{collections::HashSet, fs::File, path::{Path, PathBuf}};

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
    pub fn len(&self) -> usize {
        self.items.len()
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
            plan: plan,
        }
    }
    pub fn move_to(&self, directory: &Path) -> Planned<FileSequence> {
        let mut plan = OperationPlan::new();
        let mut new_items = Vec::new();

        for item in &self.items {
            let result = item.move_to(components, directory)
        }

        ()
    }
}
