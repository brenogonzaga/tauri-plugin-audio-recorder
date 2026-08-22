use std::path::PathBuf;

use crate::error::Error;

/// Validates that a path doesn't contain path traversal attacks.
///
/// Checks for ".." components that could escape the intended directory.
///
/// # Security
/// Always call this before writing to user-provided paths.
pub fn validate_path(path: &str) -> Result<(), Error> {
    let path_buf = PathBuf::from(path);

    for component in path_buf.components() {
        if let std::path::Component::ParentDir = component {
            return Err(Error::InvalidPath(
                "Path traversal not allowed (contains '..')".to_string(),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_normal() {
        assert!(validate_path("recording.wav").is_ok());
        assert!(validate_path("subdir/recording.wav").is_ok());
        assert!(validate_path("/absolute/path/recording.wav").is_ok());
    }

    #[test]
    fn test_validate_path_traversal() {
        assert!(validate_path("../recording.wav").is_err());
        assert!(validate_path("subdir/../recording.wav").is_err());
        assert!(validate_path("subdir/../../recording.wav").is_err());
    }
}
