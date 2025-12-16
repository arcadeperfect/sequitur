use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType{
    Rename, 
    Move,
    Copy, 
    Delete
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceExistence{
    False, Partial, True
}

/// Configuration for naming operations
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Components{
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub padding: Option<usize>,
    pub suffix: Option<String>,
    pub frame_number: Option<isize>,
}

impl Components{
    pub fn new() -> Self{
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Item {
    pub prefix: String,
    pub frame_string: String, 
    pub extention: String, 
    pub delimiter: Option<String>,
    pub suffix: Option<String>,
    pub directory: Option<PathBuf>
}

impl Item{
    pub fn frame_number(&self) -> isize{
        self.frame_string.parse().unwrap_or(0)
    }

    pub fn path(&self) -> PathBuf{
        let name = self.filename();
        match &self.directory{
            Some(dir) => dir.join(name),
            None => PathBuf::from(name),
        }
    }

    pub fn filename(&self) -> String{
        let delim = self.delimiter.as_deref().unwrap_or("");
        let suff = self.suffix.as_deref().unwrap_or("");
        format!("{}{}{}{}.{}", self.prefix, delim, self.frame_string, suff, self.extention)
    }
}

#[derive(Debug, Clone)]
pub struct FileSequence{
    items: Vec<Item>,
}

impl FileSequence{
    
    pub fn new(items: Vec<Item>) -> Self{
        let mut sorted_items = items;
        sorted_items.sort();
        Self { items: sorted_items}
    }
    
    pub fn len(&self) -> usize{
        self.items.len()
    }

        pub fn is_empty(&self) -> bool {
            self.items.is_empty()
    }
}




#[derive(Debug, Clone, PartialEq)]
pub struct FileOperation{
    pub operation: OperationType,
    pub source: PathBuf,
    pub destination: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct OperationPlan{
    pub operations: Vec<FileOperation>,
}

impl OperationPlan{
    pub fn empty() -> Self{
        Self { operations: vec![]}
    }

    pub fn has_conflicts(&self) -> bool{
        self.operations.iter().any(|op| {
            if let Some(dest) = &op.destination{
                dest.exists()
            } else {
                false
            }
        })
    }
}


