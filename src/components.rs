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

    // pub fn resolve(
    //     self,
    //     prefix: &str,
    //     delimiter: &str,
    //     padding: usize,
    //     suffix: &str,
    //     extension: &str,
    //     frame_number: i32,
    // ) -> ResolvedComponents {
    //     ResolvedComponents {
    //         prefix: self.prefix.unwrap_or_else(|| prefix.to_string()),
    //         delimiter: self.delimiter.unwrap_or_else(|| delimiter.to_string()),
    //         padding: self.padding.unwrap_or(padding),
    //         suffix: self.suffix.unwrap_or_else(|| suffix.to_string()),
    //         extension: self.extension.unwrap_or_else(|| extension.to_string()),
    //         frame_number: self.frame_number.unwrap_or(frame_number),
    //     }
    // }
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
