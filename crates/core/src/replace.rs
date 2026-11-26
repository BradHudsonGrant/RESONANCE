//! Replacing a track's file with a better copy of the same recording, and the cues that ride
//! across.

use std::cmp::Ordering;

use crate::duplicates::{Encoding, PairKind, compare_copies};
use crate::identity::MeasuredAlignment;
use crate::library::TagField;

/// What the compare sheet concludes about a new file offered for a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplaceVerdict {
    SameRecordingAndBetter,
    SameRecordingNotBetter,
    DifferentVersion,
}

impl ReplaceVerdict {
    pub const fn name(self) -> &'static str {
        match self {
            Self::SameRecordingAndBetter => "same_recording_and_better",
            Self::SameRecordingNotBetter => "same_recording_not_better",
            Self::DifferentVersion => "different_version",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SameRecordingAndBetter => "Same recording and better",
            Self::SameRecordingNotBetter => "Same recording and not better",
            Self::DifferentVersion => "Different version",
        }
    }

    /// A different version is added beside the track unless the user insists. Anything else
    /// replaces.
    pub const fn default_choice(self) -> ReplaceChoice {
        match self {
            Self::DifferentVersion => ReplaceChoice::AddSeparateTrack,
            Self::SameRecordingAndBetter | Self::SameRecordingNotBetter => ReplaceChoice::Replace,
        }
    }
}

/// What Apply does with the new file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplaceChoice {
    Replace,
    AddSeparateTrack,
}

/// Judges a new file against the track's current one. It is the same recording only when the
/// identity ladder says it is a copy and the audio lined up; a file that cannot be lined up
/// would carry cues to the wrong places, so it counts as a different version.
pub fn replace_verdict(
    kind: PairKind,
    alignment: Option<&MeasuredAlignment>,
    old: &Encoding,
    new: &Encoding,
) -> ReplaceVerdict {
    let lined_up = alignment.is_some_and(MeasuredAlignment::is_confident);
    if !kind.is_copy() || !lined_up {
        return ReplaceVerdict::DifferentVersion;
    }
    match compare_copies(new, old) {
        Ordering::Less => ReplaceVerdict::SameRecordingAndBetter,
        Ordering::Equal | Ordering::Greater => ReplaceVerdict::SameRecordingNotBetter,
    }
}

/// The tags the replacing file carries: what the user set wins, then what the old file held,
/// then what the new file brings, so nothing the library showed is lost and gaps are filled.
pub fn merge_tags(
    user: &[(TagField, String)],
    old_file: &[(TagField, String)],
    new_file: &[(TagField, String)],
) -> Vec<(TagField, String)> {
    let mut merged: Vec<(TagField, String)> = Vec::new();
    for (field, value) in user.iter().chain(old_file).chain(new_file) {
        if value.trim().is_empty() || merged.iter().any(|(taken, _)| taken == field) {
            continue;
        }
        merged.push((*field, value.clone()));
    }
    merged.sort_by_key(|(field, _)| *field);
    merged
}

/// A point the DJ marked on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CueKind {
    /// Bound to a pad.
    Hot,
    /// A marker without a pad.
    Memory,
}

impl CueKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Hot => "hot",
            Self::Memory => "memory",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "hot" => Some(Self::Hot),
            "memory" => Some(Self::Memory),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::AudioFormat;

    fn mp3_128() -> Encoding {
        Encoding::of(AudioFormat::Mp3, Some(128), Some(44_100), None, false)
    }

    fn flac() -> Encoding {
        Encoding::of(AudioFormat::Flac, Some(900), Some(44_100), Some(16), false)
    }

    fn lined_up(confidence: f32) -> MeasuredAlignment {
        MeasuredAlignment {
            new_at_old_zero_s: 0.025,
            old_per_new: 1.0,
            confidence,
        }
    }

    #[test]
    fn a_lossless_encode_of_the_same_master_is_better() {
        let verdict = replace_verdict(
            PairKind::SameRecording { similarity: 0.95 },
            Some(&lined_up(0.95)),
            &mp3_128(),
            &flac(),
        );
        assert_eq!(verdict, ReplaceVerdict::SameRecordingAndBetter);
        assert_eq!(verdict.default_choice(), ReplaceChoice::Replace);
    }

    #[test]
    fn a_worse_encode_is_the_same_recording_and_not_better() {
        let verdict = replace_verdict(
            PairKind::SameRecording { similarity: 0.95 },
            Some(&lined_up(0.95)),
            &flac(),
            &mp3_128(),
        );
        assert_eq!(verdict, ReplaceVerdict::SameRecordingNotBetter);
        let same = replace_verdict(
            PairKind::AudioIdentical,
            Some(&lined_up(1.0)),
            &flac(),
            &flac(),
        );
        assert_eq!(same, ReplaceVerdict::SameRecordingNotBetter);
    }

    #[test]
    fn a_version_or_a_file_that_will_not_line_up_is_added_beside() {
        let version = replace_verdict(
            PairKind::DifferentVersion {
                evidence: crate::duplicates::VersionEvidence::Length,
            },
            Some(&lined_up(0.95)),
            &mp3_128(),
            &flac(),
        );
        assert_eq!(version, ReplaceVerdict::DifferentVersion);
        assert_eq!(version.default_choice(), ReplaceChoice::AddSeparateTrack);
        let unaligned = replace_verdict(
            PairKind::SameRecording { similarity: 0.95 },
            Some(&lined_up(0.3)),
            &mp3_128(),
            &flac(),
        );
        assert_eq!(unaligned, ReplaceVerdict::DifferentVersion);
        let unmeasured = replace_verdict(
            PairKind::SameRecording { similarity: 0.95 },
            None,
            &mp3_128(),
            &flac(),
        );
        assert_eq!(unmeasured, ReplaceVerdict::DifferentVersion);
    }

    #[test]
    fn user_tags_win_then_the_old_file_then_the_new_one() {
        let user = vec![(TagField::Genre, "Deep house".to_owned())];
        let old = vec![
            (TagField::Title, "Night drive".to_owned()),
            (TagField::Genre, "House".to_owned()),
            (TagField::Bpm, String::new()),
        ];
        let new = vec![
            (TagField::Title, "Night Drive (Original Mix)".to_owned()),
            (TagField::Bpm, "124".to_owned()),
            (TagField::Label, "Somewhere".to_owned()),
        ];
        let merged = merge_tags(&user, &old, &new);
        let value = |field| {
            merged
                .iter()
                .find(|(taken, _)| *taken == field)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value(TagField::Genre), Some("Deep house"));
        assert_eq!(value(TagField::Title), Some("Night drive"));
        assert_eq!(value(TagField::Bpm), Some("124"));
        assert_eq!(value(TagField::Label), Some("Somewhere"));
        assert_eq!(merged.len(), 4);
    }

    #[test]
    fn cue_kinds_round_trip() {
        for kind in [CueKind::Hot, CueKind::Memory] {
            assert_eq!(CueKind::from_name(kind.name()), Some(kind));
        }
    }
}
