use crate::components::{Components, ResolvedComponents};
use crate::operation::{FileOperation, OperationPlan, Planned};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    prefix: String,
    frame_string: String,
    extension: String,
    delimiter: Option<String>,
    suffix: Option<String>,
    directory: Option<PathBuf>,
}

impl Item {
    pub fn filename(&self) -> String {
        let del = self.delimiter.as_deref().unwrap_or("");
        let suf = self.suffix.as_deref().unwrap_or("");
        format!(
            "{}{}{}{}.{}",
            self.prefix, del, self.frame_string, suf, self.extension,
        )
    }
    pub fn path(&self) -> PathBuf {
        if let Some(d) = &self.directory {
            d.join(self.filename())
        } else {
            PathBuf::from(self.filename())
        }
    }
    pub fn frame_number(&self) -> i32 {
        self.frame_string
            .parse()
            .expect("Failed to parse frame string")
    }
    pub fn padding(&self) -> usize {
        self.frame_string.len()
    }
    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    pub fn with_components(&self, components: Components, directory: Option<PathBuf>) -> Item{

        let resolved = self.resolve_components(&components);
        let target_dir = directory.or_else(|| self.directory.clone());
        Item::from_resolved(resolved, target_dir)

    }


    fn from_resolved(resolved: ResolvedComponents, directory: Option<PathBuf>) -> Self {
        let padding = resolved.padding;
        let frame_string = format!("{:0>padding$}", resolved.frame_number);
        Item {
            prefix: resolved.prefix,
            frame_string,
            extension: resolved.extension,
            delimiter: if resolved.delimiter.is_empty() {
                None
            } else {
                Some(resolved.delimiter)
            },
            suffix: if resolved.suffix.is_empty() {
                None
            } else {
                Some(resolved.suffix)
            },
            directory,
        }
    }
    pub fn resolve_components(&self, components: &Components) -> ResolvedComponents {
        ResolvedComponents {
            prefix: components
                .prefix
                .clone()
                .unwrap_or_else(|| self.prefix.clone()),
            delimiter: components
                .delimiter
                .clone()
                .unwrap_or_else(|| self.delimiter.clone().unwrap_or_default()),
            padding: components.padding.unwrap_or_else(|| self.padding()),
            suffix: components
                .suffix
                .clone()
                .unwrap_or_else(|| self.suffix.clone().unwrap_or_default()),
            extension: components
                .extension
                .clone()
                .unwrap_or_else(|| self.extension.clone()),
            frame_number: components
                .frame_number
                .unwrap_or_else(|| self.frame_number()),
        }
    }
    pub fn rename(&self, components: Components) -> Planned<Item> {
        let new_item = self.with_components(components, self.directory.clone());
        let mut plan = OperationPlan::new();
        if new_item.path() != self.path() {
            plan.push(FileOperation::Rename {
                source: self.path(),
                destination: new_item.path(),
            });
        }

        Planned {
            proposed: new_item,
            plan,
        }
    }
    pub fn delete(&self) -> OperationPlan {
        let mut plan = OperationPlan::new();
        plan.push(FileOperation::Delete {
            source: self.path(),
        });
        plan
    }

    pub fn copy_to(
        &self,
        components: Option<Components>,
        directory: Option<PathBuf>,
    ) -> Planned<Item> {

        // let target_dir = directory.or_else(||self.directory.clone());
        let new_item = if let Some(n) = components {
            // Item::from_resolved(self.resolve_components(&n), target_dir)
            self.with_components(n, directory.clone())
        } else {
            Item {
                directory: directory.or_else(||self.directory.clone()),
                ..self.clone()
            }
        };

        let mut plan = OperationPlan::new();
        plan.push(FileOperation::Copy {
            source: self.path(),
            destination: new_item.path(),
        });
        Planned{
            proposed: new_item,
            plan
        }
    }

    pub fn move_to(
        &self,
        components: Option<Components>,
        directory: Option<PathBuf>,
    ) -> Planned<Item> {

        // let target_dir = directory.or_else(||self.directory.clone());
        let new_item = if let Some(n) = components {
            // Item::from_resolved(self.resolve_components(&n), target_dir)
            self.with_components(n, directory.clone())
        } else {
            Item {
                directory: directory.or_else(||self.directory.clone()),
                ..self.clone()
            }
        };

        let mut plan = OperationPlan::new();
        plan.push(FileOperation::Move {
            source: self.path(),
            destination: new_item.path(),
        });
        Planned{
            proposed: new_item,
            plan
        }
    }
}
