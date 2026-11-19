//! Cover pictures: which image formats the library keeps and how a stored picture is named.
//!
//! A picture is stored once, named by the BLAKE3 hash of its bytes, so the twenty tracks of one
//! album share one file. Only JPEG and PNG are kept, because those are the formats DJ hardware
//! and every tag format can show.

/// The largest picture the library takes. Front covers from shops and archives are well under
/// this, and anything bigger slows every tag write that carries it.
pub const MAX_ARTWORK_BYTES: usize = 10 * 1024 * 1024;

/// The length of a stored picture's name: a BLAKE3 hash in lowercase hex.
pub const ARTWORK_HASH_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageKind {
    Jpeg,
    Png,
}

impl ImageKind {
    pub const ALL: [Self; 2] = [Self::Jpeg, Self::Png];

    /// The format the bytes start with. A file's own MIME type is not trusted, because tags in
    /// the wild often name the wrong one.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else {
            None
        }
    }

    pub const fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }

    pub fn from_mime(mime: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.mime() == mime)
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

/// Whether `name` can be a stored picture's hash. Anything else is refused before it gets near
/// a file path.
pub fn is_artwork_hash(name: &str) -> bool {
    name.len() == ARTWORK_HASH_LEN
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Why a picture is not taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtworkRefusal {
    Empty,
    TooLarge,
    NotJpegOrPng,
}

impl ArtworkRefusal {
    pub const fn message(self) -> &'static str {
        match self {
            Self::Empty => "The picture is empty",
            Self::TooLarge => "The picture is larger than 10 MB",
            Self::NotJpegOrPng => "The picture is not a JPEG or PNG",
        }
    }
}

/// The kind of a picture the library will keep, or why it will not.
pub fn check_artwork(bytes: &[u8]) -> Result<ImageKind, ArtworkRefusal> {
    if bytes.is_empty() {
        return Err(ArtworkRefusal::Empty);
    }
    if bytes.len() > MAX_ARTWORK_BYTES {
        return Err(ArtworkRefusal::TooLarge);
    }
    ImageKind::sniff(bytes).ok_or(ArtworkRefusal::NotJpegOrPng)
}

/// Standard base64 with or without padding, as a page sends a picture the user dropped. `None`
/// for anything else.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    fn value(symbol: u8) -> Option<u32> {
        match symbol {
            b'A'..=b'Z' => Some(u32::from(symbol - b'A')),
            b'a'..=b'z' => Some(u32::from(symbol - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(symbol - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let symbols = text.trim_end_matches('=').as_bytes();
    if symbols.len() % 4 == 1 || text.len() - symbols.len() > 2 {
        return None;
    }
    let mut bytes = Vec::with_capacity(symbols.len() / 4 * 3 + 2);
    for group in symbols.chunks(4) {
        let mut bits = 0u32;
        for symbol in group {
            bits = (bits << 6) | value(*symbol)?;
        }
        bits <<= 6 * (4 - group.len() as u32);
        let [_, first, second, third] = bits.to_be_bytes();
        bytes.extend_from_slice(&[first, second, third][..group.len() - 1]);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decodes_with_and_without_padding() {
        assert_eq!(decode_base64("").as_deref(), Some(&b""[..]));
        assert_eq!(decode_base64("TQ==").as_deref(), Some(&b"M"[..]));
        assert_eq!(decode_base64("TWE").as_deref(), Some(&b"Ma"[..]));
        assert_eq!(decode_base64("TWFu").as_deref(), Some(&b"Man"[..]));
        assert_eq!(
            decode_base64("/9j/4A==").as_deref(),
            Some(&[0xff, 0xd8, 0xff, 0xe0][..])
        );
        assert_eq!(decode_base64("T"), None);
        assert_eq!(decode_base64("TW-u"), None);
        assert_eq!(decode_base64("TQ==="), None);
    }

    #[test]
    fn a_picture_is_known_by_its_first_bytes() {
        assert_eq!(
            ImageKind::sniff(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(ImageKind::Jpeg)
        );
        assert_eq!(
            ImageKind::sniff(b"\x89PNG\r\n\x1a\n\0\0"),
            Some(ImageKind::Png)
        );
        assert_eq!(ImageKind::sniff(b"GIF89a"), None);
        assert_eq!(ImageKind::sniff(b""), None);
    }

    #[test]
    fn only_lowercase_hex_of_the_right_length_is_a_hash() {
        assert!(is_artwork_hash(&"a1".repeat(32)));
        assert!(!is_artwork_hash(&"A1".repeat(32)));
        assert!(!is_artwork_hash(&"a1".repeat(31)));
        assert!(!is_artwork_hash("../../library.db"));
        assert!(!is_artwork_hash(&format!("{}.jpg", "a".repeat(60))));
    }

    #[test]
    fn empty_large_and_other_formats_are_refused() {
        assert_eq!(check_artwork(&[]), Err(ArtworkRefusal::Empty));
        assert_eq!(check_artwork(b"GIF89a"), Err(ArtworkRefusal::NotJpegOrPng));
        let mut large = vec![0xff, 0xd8, 0xff];
        large.resize(MAX_ARTWORK_BYTES + 1, 0);
        assert_eq!(check_artwork(&large), Err(ArtworkRefusal::TooLarge));
        large.truncate(MAX_ARTWORK_BYTES);
        assert_eq!(check_artwork(&large), Ok(ImageKind::Jpeg));
    }
}
