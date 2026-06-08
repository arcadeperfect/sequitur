#[derive(Debug, Clone, Default, PartialEq)]
pub struct Components {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub padding: Option<usize>,
    pub suffix: Option<String>,
    pub extension: Option<String>,
    pub frame_number: Option<i32>,
}

impl Components {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    pub fn delimiter(mut self, delimiter: impl Into<String>) -> Self {
        self.delimiter = Some(delimiter.into());
        self
    }

    pub fn padding(mut self, padding: usize) -> Self {
        self.padding = Some(padding);
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    pub fn extension(mut self, extension: impl Into<String>) -> Self {
        self.extension = Some(extension.into());
        self
    }

    pub fn frame_number(mut self, frame_number: i32) -> Self {
        self.frame_number = Some(frame_number);
        self
    }

    pub fn with_frame_number(&self, frame_number: i32) -> Components {
        Components {
            frame_number: Some(frame_number),
            ..self.clone()
        }
    }

    /// Parses a sequence-string pattern into `Components`.
    ///
    /// Accepts both hash notation (`render_####.exr`) and printf notation
    /// (`render_%04d.exr`); the frame placeholder's width becomes the padding.
    /// The resulting `Components` has no `frame_number` (the pattern describes
    /// a whole sequence, not one frame). Returns `None` if there is no `#` /
    /// `%d` placeholder.
    ///
    /// # Examples
    ///
    /// ```
    /// use sequitur::Components;
    /// let c = Components::from_sequence_string("render_####.exr").unwrap();
    /// assert_eq!(c.prefix.as_deref(), Some("render"));
    /// assert_eq!(c.delimiter.as_deref(), Some("_"));
    /// assert_eq!(c.padding, Some(4));
    /// assert_eq!(c.extension.as_deref(), Some("exr"));
    /// ```
    pub fn from_sequence_string(pattern: &str) -> Option<Components> {
        let normalized = convert_padding_to_hashes(pattern);
        let (name_part, extension) = crate::item::split_extension(&normalized);

        let bytes = name_part.as_bytes();
        let hash_end = bytes.iter().rposition(|&b| b == b'#')? + 1;
        let mut hash_start = hash_end;
        while hash_start > 0 && bytes[hash_start - 1] == b'#' {
            hash_start -= 1;
        }

        let padding = hash_end - hash_start;
        let suffix = &name_part[hash_end..];
        let (prefix, delimiter) = crate::item::split_prefix_delimiter(&name_part[..hash_start]);

        let mut components = Components::new()
            .prefix(prefix)
            .padding(padding)
            .extension(extension);
        if let Some(d) = delimiter {
            components = components.delimiter(d);
        }
        if !suffix.is_empty() {
            components = components.suffix(suffix);
        }
        Some(components)
    }
}

/// Converts printf-style frame patterns (`%04d`, `%d`) into hash notation
/// (`####`, `#`). Other text is left untouched.
pub fn convert_padding_to_hashes(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j < chars.len() && chars[j] == 'd' {
                let digits: String = chars[i + 1..j].iter().collect();
                let count = if digits.is_empty() {
                    1
                } else {
                    digits.parse::<usize>().unwrap_or(1)
                };
                out.push_str(&"#".repeat(count));
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedComponents {
    pub prefix: String,
    pub delimiter: String,
    pub padding: usize,
    pub suffix: String,
    pub extension: String,
    pub frame_number: i32,
}
