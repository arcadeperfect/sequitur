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

    pub fn prefix(mut self, prefix: impl Into<String>) -> Self{
        self.prefix = Some(prefix.into());
        self
    }

    pub fn delimiter(mut self, delimiter: impl Into<String>) -> Self{
        self.delimiter = Some(delimiter.into());
        self
    }

    pub fn padding(mut self, padding: usize) -> Self{
        self.padding = Some(padding);
        self
    }

    pub fn suffix(mut self, suffix: impl Into<String>) -> Self{
        self.suffix = Some(suffix.into());
        self
    }

    pub fn extension(mut self, extension: impl Into<String>) -> Self{
        self.extension = Some(extension.into());
        self
    }

    pub fn frame_number(mut self, frame_number: i32) -> Self{
        self.frame_number = Some(frame_number);
        self
    }

}