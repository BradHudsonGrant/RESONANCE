use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionNameError {
    Empty,
    ControlCharacter,
}

impl fmt::Display for CollectionNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Collection name cannot be empty."),
            Self::ControlCharacter => {
                f.write_str("Collection name cannot contain line breaks or control characters.")
            }
        }
    }
}

impl std::error::Error for CollectionNameError {}

/// Validates and trims a user-supplied collection name.
pub fn normalise_collection_name(raw: &str) -> Result<String, CollectionNameError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CollectionNameError::Empty);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(CollectionNameError::ControlCharacter);
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_trimmed_names_and_rejects_blank_or_control_input() {
        assert_eq!(
            normalise_collection_name("  Warm Up \t ").as_deref(),
            Ok("Warm Up")
        );
        assert_eq!(
            normalise_collection_name("Café Terrace").as_deref(),
            Ok("Café Terrace")
        );
        assert_eq!(
            normalise_collection_name("   \t "),
            Err(CollectionNameError::Empty)
        );
        assert_eq!(
            normalise_collection_name("Warm\nUp"),
            Err(CollectionNameError::ControlCharacter)
        );
    }
}
