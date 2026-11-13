use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Minor,
    Major,
}

/// One of the 24 keys, held as its place on the Camelot wheel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    hour: u8,
    mode: Mode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyParseError {
    input: String,
}

impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\"{}\" is not a key. Use a Camelot code such as 8A or a name such as F#m.",
            self.input
        )
    }
}

impl std::error::Error for KeyParseError {}

const MINOR_NAMES: [&str; 12] = [
    "Abm", "Ebm", "Bbm", "Fm", "Cm", "Gm", "Dm", "Am", "Em", "Bm", "F#m", "Dbm",
];
const MAJOR_NAMES: [&str; 12] = [
    "B", "F#", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E",
];

impl Key {
    pub fn hour(self) -> u8 {
        self.hour
    }

    pub fn mode(self) -> Mode {
        self.mode
    }

    /// The Camelot code, such as `8A`. A is the minor ring and B the major ring.
    pub fn camelot(self) -> String {
        let ring = match self.mode {
            Mode::Minor => 'A',
            Mode::Major => 'B',
        };
        format!("{}{ring}", self.hour)
    }

    /// The musical name DJ software shows for this key, leaning to flats.
    pub fn name(self) -> &'static str {
        let names = match self.mode {
            Mode::Minor => &MINOR_NAMES,
            Mode::Major => &MAJOR_NAMES,
        };
        names[usize::from(self.hour - 1)]
    }

    pub fn all() -> impl Iterator<Item = Key> {
        [Mode::Minor, Mode::Major]
            .into_iter()
            .flat_map(|mode| (1..=12).map(move |hour| Key { hour, mode }))
    }

    fn from_camelot(text: &str) -> Option<Key> {
        let ring = text.chars().last()?;
        let mode = match ring {
            'A' | 'a' => Mode::Minor,
            'B' | 'b' => Mode::Major,
            _ => return None,
        };
        let digits = &text[..text.len() - ring.len_utf8()];
        if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let hour: u8 = digits.parse().ok()?;
        (1..=12).contains(&hour).then_some(Key { hour, mode })
    }

    fn from_name(text: &str) -> Option<Key> {
        let mut chars = text.chars();
        let semitone: i8 = match chars.next()?.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest = chars.as_str();
        let (shift, rest) = match rest.chars().next() {
            Some('#' | '♯') => (1, &rest[rest.chars().next()?.len_utf8()..]),
            Some('b' | '♭') => (-1, &rest[rest.chars().next()?.len_utf8()..]),
            _ => (0, rest),
        };
        let mode = mode_from_suffix(rest.trim_start_matches([' ', '-', '_']))?;
        let pitch = (semitone + shift).rem_euclid(12);
        let tonic_of_major = match mode {
            Mode::Major => pitch,
            Mode::Minor => (pitch + 3).rem_euclid(12),
        };
        // Each step round the wheel is a fifth, and seven semitones is a fifth.
        let steps = (i16::from(tonic_of_major) * 7).rem_euclid(12);
        let hour = u8::try_from((steps + 7).rem_euclid(12) + 1).ok()?;
        Some(Key { hour, mode })
    }
}

/// A lone capital M means major and a lone small m means minor, as DJ software writes them.
fn mode_from_suffix(suffix: &str) -> Option<Mode> {
    match suffix {
        "" | "M" => return Some(Mode::Major),
        "m" => return Some(Mode::Minor),
        _ => {}
    }
    match suffix.to_ascii_lowercase().as_str() {
        "maj" | "major" => Some(Mode::Major),
        "min" | "minor" => Some(Mode::Minor),
        _ => None,
    }
}

impl FromStr for Key {
    type Err = KeyParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let text = input.trim();
        Key::from_camelot(text)
            .or_else(|| Key::from_name(text))
            .ok_or_else(|| KeyParseError {
                input: input.to_owned(),
            })
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.camelot(), self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camelot_of(text: &str) -> String {
        text.parse::<Key>().unwrap().camelot()
    }

    #[test]
    fn c_major_is_8b_and_a_minor_is_8a() {
        assert_eq!(camelot_of("C"), "8B");
        assert_eq!(camelot_of("Am"), "8A");
    }

    #[test]
    fn camelot_codes_parse_in_any_case_and_with_a_leading_zero() {
        assert_eq!(camelot_of("8a"), "8A");
        assert_eq!(camelot_of("08A"), "8A");
        assert_eq!(camelot_of("12b"), "12B");
        assert_eq!(camelot_of(" 1A "), "1A");
    }

    #[test]
    fn musical_names_parse_in_the_spellings_tags_use() {
        assert_eq!(camelot_of("Abm"), "1A");
        assert_eq!(camelot_of("G# minor"), "1A");
        assert_eq!(camelot_of("F#min"), "11A");
        assert_eq!(camelot_of("Db maj"), "3B");
        assert_eq!(camelot_of("A Major"), "11B");
        assert_eq!(camelot_of("f#m"), "11A");
        assert_eq!(camelot_of("B♭"), "6B");
        assert_eq!(camelot_of("C♯m"), "12A");
        assert_eq!(camelot_of("E-minor"), "9A");
    }

    #[test]
    fn a_lone_capital_m_is_major_and_a_lone_small_m_is_minor() {
        assert_eq!(camelot_of("AM"), "11B");
        assert_eq!(camelot_of("Am"), "8A");
    }

    #[test]
    fn enharmonic_spellings_are_the_same_key() {
        for (sharp, flat) in [
            ("G#m", "Abm"),
            ("D#m", "Ebm"),
            ("A#m", "Bbm"),
            ("C#", "Db"),
            ("F#", "Gb"),
            ("B", "Cb"),
        ] {
            assert_eq!(
                sharp.parse::<Key>().unwrap(),
                flat.parse::<Key>().unwrap(),
                "{sharp} and {flat}"
            );
        }
    }

    #[test]
    fn every_key_reads_back_from_its_own_code_and_name() {
        for key in Key::all() {
            assert_eq!(key.camelot().parse::<Key>().unwrap(), key);
            assert_eq!(key.name().parse::<Key>().unwrap(), key);
        }
        assert_eq!(Key::all().count(), 24);
    }

    #[test]
    fn relative_major_and_minor_share_an_hour() {
        for (minor, major) in [("Am", "C"), ("Em", "G"), ("F#m", "A"), ("Cm", "Eb")] {
            let (minor, major): (Key, Key) = (minor.parse().unwrap(), major.parse().unwrap());
            assert_eq!(minor.hour(), major.hour());
            assert_eq!((minor.mode(), major.mode()), (Mode::Minor, Mode::Major));
        }
    }

    #[test]
    fn text_that_is_not_a_key_is_refused() {
        for junk in ["", "13A", "0B", "8C", "H", "Xm", "Am7", "128", "A sharp"] {
            assert!(junk.parse::<Key>().is_err(), "{junk:?} should not parse");
        }
    }

    #[test]
    fn display_shows_the_code_and_the_name() {
        assert_eq!("F#m".parse::<Key>().unwrap().to_string(), "11A (F#m)");
    }
}
