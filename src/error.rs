use std::error::Error;

#[derive(Debug)]
pub enum HexisError {
    Custom(String),
}

impl Error for HexisError {}

impl std::fmt::Display for HexisError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            HexisError::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<String> for HexisError {
    fn from(msg: String) -> Self {
        HexisError::Custom(msg)
    }
}
