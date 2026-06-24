use std::fmt;

#[derive(Debug)]
pub enum DiavloError {
    AudioDevice(String),
    Decode(String),
    UnsupportedFormat(String),
    FileNotFound(String),
    Config(String),
}

impl std::error::Error for DiavloError {}

impl fmt::Display for DiavloError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiavloError::AudioDevice(msg) => write!(f, "Audio device error: {}", msg),
            DiavloError::Decode(msg) => write!(f, "Decode error: {}", msg),
            DiavloError::UnsupportedFormat(msg) => write!(f, "Unsupported format: {}", msg),
            DiavloError::FileNotFound(msg) => write!(f, "File not found: {}", msg),
            DiavloError::Config(msg) => write!(f, "Config error: {}", msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, DiavloError>;
