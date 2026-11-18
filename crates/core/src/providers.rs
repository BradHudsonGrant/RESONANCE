use std::fmt;

/// The name every provider key is filed under in the OS credential store.
pub const CREDENTIAL_SERVICE: &str = "com.resonance.dj";

/// Long enough for any provider token seen so far, short enough to reject a pasted file.
pub const MAX_KEY_LENGTH: usize = 512;

/// A key or token a provider needs, entered by the user in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKey {
    /// AcoustID application key, for fingerprint lookups.
    AcoustId,
    /// Discogs personal access token, for release search.
    Discogs,
    /// AudD API token, a paid recogniser.
    AudD,
}

impl ProviderKey {
    pub const ALL: [Self; 3] = [Self::AcoustId, Self::Discogs, Self::AudD];

    /// The account name the key is stored under, stable across releases.
    pub fn account(self) -> &'static str {
        match self {
            Self::AcoustId => "acoustid",
            Self::Discogs => "discogs",
            Self::AudD => "audd",
        }
    }

    /// Paid recognisers sit behind their own disclosure in Settings.
    pub fn is_paid(self) -> bool {
        matches!(self, Self::AudD)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRejected {
    Empty,
    TooLong,
    ControlCharacter,
}

impl fmt::Display for KeyRejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "The key is empty. Paste the key, then save again.",
            Self::TooLong => "The key is longer than any provider issues. Check what was pasted.",
            Self::ControlCharacter => {
                "The key contains a line break or control character. Paste it on one line."
            }
        })
    }
}

impl std::error::Error for KeyRejected {}

/// Trims what the user pasted and checks it could be a key. The value itself never appears in
/// the rejection, so it cannot leak into a message or a log.
pub fn normalise_key(value: &str) -> Result<&str, KeyRejected> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(KeyRejected::Empty);
    }
    if trimmed.len() > MAX_KEY_LENGTH {
        return Err(KeyRejected::TooLong);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(KeyRejected::ControlCharacter);
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_are_distinct() {
        let mut accounts: Vec<_> = ProviderKey::ALL.iter().map(|key| key.account()).collect();
        accounts.sort_unstable();
        accounts.dedup();
        assert_eq!(accounts.len(), ProviderKey::ALL.len());
    }

    #[test]
    fn only_recognisers_are_paid() {
        assert!(!ProviderKey::AcoustId.is_paid());
        assert!(!ProviderKey::Discogs.is_paid());
        assert!(ProviderKey::AudD.is_paid());
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(normalise_key("  abc123 \n"), Ok("abc123"));
    }

    #[test]
    fn rejects_empty_long_and_multiline_keys() {
        assert_eq!(normalise_key("   "), Err(KeyRejected::Empty));
        assert_eq!(
            normalise_key(&"k".repeat(MAX_KEY_LENGTH + 1)),
            Err(KeyRejected::TooLong)
        );
        assert_eq!(
            normalise_key("abc\ndef"),
            Err(KeyRejected::ControlCharacter)
        );
    }

    #[test]
    fn rejection_never_repeats_the_value() {
        let secret = "very-secret-value";
        let message = normalise_key(&format!("{secret}\t{secret}"))
            .err()
            .map(|rejected| rejected.to_string())
            .unwrap_or_default();
        assert!(!message.contains(secret));
    }
}
