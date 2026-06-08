use crate::components::{Components, ResolvedComponents};
use crate::operation::{FileOperation, OperationPlan, Planned};
use std::path::{Path, PathBuf};

/// Compound extensions that should be treated as a single extension rather than
/// split on the final dot (e.g. `archive.tar.gz` has extension `tar.gz`).
const KNOWN_EXTENSIONS: &[&str] = &["tar.gz", "tar.bz2", "log.gz"];

/// Represents a single item in a file sequence.
///
/// An `Item` models files that follow a naming pattern commonly used in
/// visual effects and animation pipelines, such as image sequences:
///
/// ```text
/// prefix[delimiter]frame[suffix].extension
/// ```
///
/// # Examples
///
/// Common file patterns this represents:
/// - `render.0001.exr` (prefix: "render", delimiter: ".", frame: "0001", extension: "exr")
/// - `shot_010_v2_0042.png` (prefix: "shot_010_v2_", frame: "0042", extension: "png")
/// - `frame0001_final.jpg` (prefix: "frame", frame: "0001", suffix: "_final", extension: "jpg")
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Item {
    /// The base name before the frame number (e.g., "render" in "render.0001.exr")
    prefix: String,
    /// The frame number as a zero-padded string (e.g., "0001")
    frame_string: String,
    /// The file extension without the dot (e.g., "exr")
    extension: String,
    /// Optional separator between prefix and frame (e.g., "." or "_")
    delimiter: Option<String>,
    /// Optional text after the frame number but before the extension
    suffix: Option<String>,
    /// The directory containing this item, if known
    directory: Option<PathBuf>,
}

impl Item {
    /// Parses a single filename into an `Item`.
    ///
    /// The filename is split into `prefix[delimiter]frame[suffix].extension` by
    /// locating the **last** run of digits in the name and treating it as the
    /// frame number. Returns `None` if the name contains no digits (and thus is
    /// not part of a numbered sequence).
    ///
    /// # Arguments
    ///
    /// * `filename` - A bare filename (not a path); e.g. `"render_001.exr"`
    /// * `directory` - Optional directory the file lives in
    ///
    /// # Examples
    ///
    /// ```
    /// use sequitur::Item;
    /// let item = Item::from_filename("render_001.exr", None).unwrap();
    /// assert_eq!(item.prefix(), "render");
    /// assert_eq!(item.delimiter(), Some("_"));
    /// assert_eq!(item.frame_string(), "001");
    /// assert_eq!(item.extension(), "exr");
    /// ```
    pub fn from_filename(filename: &str, directory: Option<PathBuf>) -> Option<Item> {
        let (name_part, extension) = split_extension(filename);

        // The frame number is the last maximal run of ASCII digits in name_part.
        let bytes = name_part.as_bytes();
        let frame_end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
        let mut frame_start = frame_end;
        while frame_start > 0 && bytes[frame_start - 1].is_ascii_digit() {
            frame_start -= 1;
        }

        let frame_string = name_part[frame_start..frame_end].to_string();
        let suffix = &name_part[frame_end..];

        // Split the text before the frame into prefix + delimiter.
        let (prefix, delimiter) = split_prefix_delimiter(&name_part[..frame_start]);

        Some(Item {
            prefix,
            frame_string,
            extension: extension.to_string(),
            delimiter,
            suffix: if suffix.is_empty() {
                None
            } else {
                Some(suffix.to_string())
            },
            directory,
        })
    }

    /// Parses a [`Path`] into an `Item`, using its file name and parent
    /// directory. Returns `None` if the path has no file name or the name
    /// contains no digits.
    pub fn from_path(path: &Path) -> Option<Item> {
        let filename = path.file_name()?.to_str()?;
        let directory = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => Some(p.to_path_buf()),
            _ => None,
        };
        Item::from_filename(filename, directory)
    }

    /// Returns the prefix portion of the filename.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Returns the frame number as a zero-padded string (e.g., "0001").
    pub fn frame_string(&self) -> &str {
        &self.frame_string
    }

    /// Returns the file extension without the leading dot.
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// Returns the delimiter between the prefix and frame number, if present.
    pub fn delimiter(&self) -> Option<&str> {
        self.delimiter.as_deref()
    }

    /// Returns the suffix after the frame number, if present.
    pub fn suffix(&self) -> Option<&str> {
        self.suffix.as_deref()
    }

    /// Returns the directory containing this item, if known.
    pub fn directory(&self) -> Option<&PathBuf> {
        self.directory.as_ref()
    }

    /// Constructs the complete filename from all components.
    ///
    /// # Example
    ///
    /// For an item with prefix "render", delimiter ".", frame "0001", and extension "exr":
    /// ```text
    /// item.filename() // => "render.0001.exr"
    /// ```
    pub fn filename(&self) -> String {
        let del = self.delimiter.as_deref().unwrap_or("");
        let suf = self.suffix.as_deref().unwrap_or("");
        format!(
            "{}{}{}{}.{}",
            self.prefix, del, self.frame_string, suf, self.extension,
        )
    }

    /// Returns the full path to this item, including the directory if set.
    pub fn path(&self) -> PathBuf {
        if let Some(d) = &self.directory {
            d.join(self.filename())
        } else {
            PathBuf::from(self.filename())
        }
    }

    /// Parses and returns the frame number as an integer.
    ///
    /// # Panics
    ///
    /// Panics if the frame string cannot be parsed as an `i32`.
    pub fn frame_number(&self) -> i32 {
        self.frame_string
            .parse()
            .expect("Failed to parse frame string")
    }

    /// Returns the zero-padding width of the frame number.
    ///
    /// For example, "0001" has a padding of 4.
    pub fn padding(&self) -> usize {
        self.frame_string.len()
    }

    /// Checks whether the file exists on disk.
    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    /// Creates a new `Item` by applying the given components, using this item's
    /// values as defaults for any unspecified components.
    ///
    /// # Arguments
    ///
    /// * `components` - The components to apply (unset fields use current values)
    /// * `directory` - Optional target directory; falls back to current directory if `None`
    pub fn with_components(&self, components: Components, directory: Option<PathBuf>) -> Item {
        let resolved = self.resolve_components(&components);
        let target_dir = directory.or_else(|| self.directory.clone());
        Item::from_resolved(resolved, target_dir)
    }

    /// Constructs an `Item` from fully resolved components.
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

    /// Resolves partial components against this item's current values.
    ///
    /// Any `None` fields in `components` will be filled with the corresponding
    /// values from this item.
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

    /// Creates a rename operation plan for this item.
    ///
    /// Returns a [`Planned<Item>`] containing the proposed new item and the
    /// operations needed to rename the file. If the resulting path is unchanged,
    /// no operations are added to the plan.
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

    /// Creates a delete operation plan for this item.
    pub fn delete(&self) -> OperationPlan {
        let mut plan = OperationPlan::new();
        plan.push(FileOperation::Delete {
            source: self.path(),
        });
        plan
    }

    /// Creates a copy operation plan for this item.
    ///
    /// # Arguments
    ///
    /// * `components` - Optional components to modify for the destination filename
    /// * `directory` - Optional target directory for the copy
    ///
    /// # Returns
    ///
    /// A [`Planned<Item>`] containing the proposed destination item and the
    /// copy operation.
    pub fn copy_to(
        &self,
        components: Option<Components>,
        directory: Option<PathBuf>,
    ) -> Planned<Item> {
        let new_item = self.build_new_item(components, directory);

        let mut plan = OperationPlan::new();
        plan.push(FileOperation::Copy {
            source: self.path(),
            destination: new_item.path(),
        });
        Planned {
            proposed: new_item,
            plan,
        }
    }

    /// Creates a move operation plan for this item.
    ///
    /// # Arguments
    ///
    /// * `components` - Optional components to modify for the destination filename
    /// * `directory` - Optional target directory for the move
    ///
    /// # Returns
    ///
    /// A [`Planned<Item>`] containing the proposed destination item and the
    /// move operation.
    pub fn move_to(
        &self,
        components: Option<Components>,
        directory: Option<PathBuf>,
    ) -> Planned<Item> {
        let new_item = self.build_new_item(components, directory);

        let mut plan = OperationPlan::new();
        plan.push(FileOperation::Move {
            source: self.path(),
            destination: new_item.path(),
        });
        Planned {
            proposed: new_item,
            plan,
        }
    }

    /// Creates a rename plan to change the frame number.
    ///
    /// # Arguments
    ///
    /// * `frame_number` - The new frame number
    /// * `padding` - Optional padding width; uses current padding if `None`
    pub fn with_frame_number(&self, frame_number: i32, padding: Option<usize>) -> Planned<Item> {
        let padding: usize = padding.unwrap_or_else(|| self.padding());
        let components = Components::new()
            .frame_number(frame_number)
            .padding(padding);
        self.rename(components)
    }

    /// Creates a rename plan to change the zero-padding width.
    ///
    /// # Arguments
    ///
    /// * `padding` - The new padding width (e.g., 4 for "0001")
    pub fn with_padding(&self, padding: usize) -> Planned<Item> {
        let components = Components::new().padding(padding);
        self.rename(components)
    }

    /// Helper to build a new item with optional component and directory changes.
    fn build_new_item(&self, components: Option<Components>, directory: Option<PathBuf>) -> Item {
        match components {
            Some(c) => self.with_components(c, directory),
            None => Item {
                directory: directory.or_else(|| self.directory.clone()),
                ..self.clone()
            },
        }
    }
}

/// Splits a filename into its name part and extension, honouring known
/// compound extensions such as `tar.gz`. Returns `(name_part, extension)` where
/// `extension` excludes the leading dot and may be empty.
pub(crate) fn split_extension(filename: &str) -> (&str, &str) {
    for ext in KNOWN_EXTENSIONS {
        if let Some(stem) = filename
            .strip_suffix(ext)
            .and_then(|s| s.strip_suffix('.'))
        {
            return (stem, ext);
        }
    }
    match filename.rfind('.') {
        Some(dot) => (&filename[..dot], &filename[dot + 1..]),
        None => (filename, ""),
    }
}

/// Splits the text that precedes a frame number (or `####` placeholder) into a
/// `(prefix, delimiter)` pair.
///
/// The delimiter is the run of non-alphanumeric chars immediately before the
/// frame; only its final char is kept as the delimiter, and any earlier chars
/// fold back into the prefix (mirroring pysequitur's `len(delimiter) > 1`
/// rule). Walks by chars, not bytes, to stay UTF-8 safe.
pub(crate) fn split_prefix_delimiter(before_frame: &str) -> (String, Option<String>) {
    let mut delim_start = before_frame.len();
    for (idx, ch) in before_frame.char_indices().rev() {
        if is_delimiter_char(ch) {
            delim_start = idx;
        } else {
            break;
        }
    }

    if delim_start == before_frame.len() {
        (before_frame.to_string(), None)
    } else {
        let last_char_start = before_frame
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        (
            before_frame[..last_char_start].to_string(),
            Some(before_frame[last_char_start..].to_string()),
        )
    }
}

/// A char is a delimiter candidate if it is neither an ASCII letter nor an
/// ASCII digit (mirrors the regex class `[^a-zA-Z\d]`). Underscore counts, as
/// do non-ASCII letters such as `é`.
fn is_delimiter_char(c: char) -> bool {
    !c.is_ascii_alphanumeric()
}
