use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};

/// Made-up tracks with plausible tags, for measuring the library at sizes no real folder here has.
/// The same count always gives the same tracks.
pub fn tracks(count: usize) -> Vec<ScannedFile> {
    let mut dice = Dice::new(0x9E37_79B9_7F4A_7C15);
    let mut tracks = Vec::with_capacity(count);

    for i in 0..count {
        let artist_first = dice.pick(&ARTIST_FIRST);
        let artist_second = dice.pick(&ARTIST_SECOND);
        let artist = format!("{artist_first} {artist_second}");

        let title_word_count = if dice.below(2) == 0 { 2 } else { 3 };
        let mut title_words = Vec::with_capacity(title_word_count);
        for _ in 0..title_word_count {
            title_words.push(dice.pick(&TITLE_WORDS));
        }
        let title = title_words
            .iter()
            .map(|word| title_case(word))
            .collect::<Vec<_>>()
            .join(" ");

        let format = AudioFormat::ALL[i % 5];
        let genre = dice.pick(&GENRES);
        let label = dice.pick(&LABELS);

        let duration_ms = 150_000 + dice.below(390_001);
        let sample_rate: u32 = if dice.below(2) == 0 { 44_100 } else { 48_000 };
        let channels = 2;

        let bit_depth = match format {
            AudioFormat::Mp3 | AudioFormat::M4a => None,
            _ => Some(if dice.below(2) == 0 { 16 } else { 24 }),
        };

        let bitrate_kbps = match format {
            AudioFormat::Mp3 => Some(320),
            AudioFormat::M4a => Some(256),
            _ => Some(sample_rate * u32::from(bit_depth.unwrap_or(16)) * 2 / 1000),
        };

        let audio = AudioFacts {
            duration_ms,
            sample_rate: Some(sample_rate),
            channels: Some(channels),
            bit_depth,
            bitrate_kbps,
        };

        let ext = format.name();
        let path = format!("{genre}/{artist}/{i:06} {artist} - {title}.{ext}");

        let size_bytes = duration_ms / 1000 * u64::from(bitrate_kbps.unwrap_or(0)) * 125;
        let modified_ns = 1_700_000_000_000_000_000 + (i as i64) * 1_000_000_007;

        let year = 1990 + dice.below(37);
        let bpm_whole = 80 + dice.below(96);
        let bpm = if i % 7 == 0 {
            format!("{bpm_whole}.50")
        } else {
            bpm_whole.to_string()
        };

        let camelot_number = 1 + dice.below(12);
        let camelot_letter = if dice.below(2) == 0 { 'A' } else { 'B' };

        let album = format!(
            "{} {} EP",
            title_case(title_words[0]),
            title_case(title_words[1])
        );

        let mut tags = vec![
            (TagField::Title, title.clone()),
            (TagField::Artist, artist.clone()),
            (TagField::Album, album),
            (TagField::Year, year.to_string()),
            (TagField::Bpm, bpm),
        ];

        // Some tracks lack a genre or a label, as in a real library, so filters for missing tags
        // have something to find.
        if i % 9 != 0 {
            tags.push((TagField::Genre, genre.to_string()));
        }

        if i % 11 != 0 {
            tags.push((TagField::Key, format!("{camelot_number}{camelot_letter}")));
        }

        if i % 13 != 0 {
            tags.push((TagField::Label, label.to_string()));
        }

        // Tags the table can add as columns, on some tracks and not others, so sorts by them
        // have empty values to put last and wrapped rows have something to wrap.
        if i % 5 != 0 {
            tags.push((TagField::TrackNumber, (i % 12 + 1).to_string()));
        }
        if i % 3 != 0 {
            tags.push((TagField::CatNo, format!("SYN{:04}", i % 4999)));
        }
        if i % 2 == 0 {
            tags.push((TagField::Composer, artist.clone()));
        }
        if i % 6 == 0 {
            tags.push((TagField::Mix, "Extended Mix".to_owned()));
        }
        if i % 4 == 0 {
            tags.push((
                TagField::Comment,
                format!(
                    "Warm-up pick from crate {}, good after a long breakdown",
                    i % 40
                ),
            ));
        }

        tracks.push(ScannedFile {
            path,
            size_bytes,
            modified_ns,
            format,
            audio: Some(audio),
            tags,
        });
    }

    tracks
}

struct Dice {
    state: u64,
}

impl Dice {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

const ARTIST_FIRST: [&str; 24] = [
    "Vela", "Kiro", "Milo", "Zara", "Nico", "Luna", "Rex", "Ivy", "Jax", "Ora", "Pax", "Quin",
    "Rho", "Sia", "Tao", "Uma", "Vex", "Wren", "Xen", "Yara", "Zed", "Aro", "Bex", "Cyr",
];

const ARTIST_SECOND: [&str; 24] = [
    "Hollow", "Bright", "Dark", "Soft", "Sharp", "Wild", "Calm", "Bold", "Deep", "High", "Low",
    "Fast", "Slow", "New", "Old", "True", "False", "Pure", "Raw", "Clean", "Dirty", "Warm", "Cool",
    "Hot",
];

const TITLE_WORDS: [&str; 40] = [
    "night",
    "day",
    "light",
    "dark",
    "fire",
    "ice",
    "wind",
    "rain",
    "sun",
    "moon",
    "star",
    "cloud",
    "storm",
    "thunder",
    "lightning",
    "ocean",
    "river",
    "mountain",
    "valley",
    "forest",
    "desert",
    "city",
    "street",
    "road",
    "bridge",
    "tower",
    "castle",
    "garden",
    "field",
    "meadow",
    "hill",
    "dune",
    "cave",
    "creek",
    "lake",
    "bay",
    "coast",
    "shore",
    "beach",
    "island",
];

const GENRES: [&str; 16] = [
    "House",
    "Deep House",
    "Tech House",
    "Techno",
    "Garage",
    "UK Garage",
    "Drum & Bass",
    "Breaks",
    "Disco",
    "Nu Disco",
    "Electro",
    "Dubstep",
    "Jungle",
    "Grime",
    "Bass",
    "Minimal",
];

const LABELS: [&str; 12] = [
    "Neon Pulse",
    "Static Bloom",
    "Echo Drift",
    "Void Signal",
    "Circuit Flow",
    "Ghost Wave",
    "Iron Beat",
    "Silent Storm",
    "Crystal Echo",
    "Shadow Grid",
    "Lunar Code",
    "Solar Byte",
];

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_count_gives_the_same_tracks() {
        let a = tracks(100);
        let b = tracks(100);
        assert_eq!(a, b);
    }

    #[test]
    fn every_track_has_its_own_path() {
        let t = tracks(1000);
        let mut paths: Vec<&str> = t.iter().map(|f| f.path.as_str()).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), 1000);
    }

    #[test]
    fn every_path_ends_in_the_extension_of_its_format() {
        for f in tracks(500) {
            assert!(f.path.ends_with(f.format.name()));
        }
    }
}
