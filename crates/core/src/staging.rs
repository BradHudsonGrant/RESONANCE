//! The three tag layers and the staging diff between them.
//!
//! A file's own tags, values identification proposes, and values the user typed or accepted.
//! What a track shows is the user's value where there is one and the file's otherwise. Nothing
//! identification does ever changes a user value: a proposal for a field the user holds is shown
//! as locked and is never accepted in bulk.

use std::fmt;

use crate::identify::RankedCandidate;
use crate::key::Key;
use crate::library::TagField;

/// The fields staging compares, in the order it shows them.
pub const STAGED_FIELDS: [TagField; 13] = [
    TagField::Title,
    TagField::Artist,
    TagField::Mix,
    TagField::Album,
    TagField::Genre,
    TagField::Subgenre,
    TagField::Label,
    TagField::CatNo,
    TagField::Year,
    TagField::Bpm,
    TagField::Key,
    TagField::TrackNumber,
    TagField::Comment,
];

/// Which layer a track's value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    File,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Skipped,
}

impl ProposalStatus {
    pub const ALL: [Self; 3] = [Self::Pending, Self::Accepted, Self::Skipped];

    pub fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.name() == name)
    }
}

/// One field's value as identification found it.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub field: TagField,
    pub value: String,
    pub provider: String,
    pub confidence: f64,
    pub audio_matched: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredProposal {
    pub proposal: Proposal,
    pub status: ProposalStatus,
}

/// A value that cannot be stored for its field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagValueError {
    NotAKey(String),
    NotABpm(String),
    NotAYear(String),
}

impl fmt::Display for TagValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAKey(value) => write!(
                f,
                "\"{value}\" is not a key. Use a Camelot code such as 8A, an Open Key code such \
                 as 1m or a name such as Am."
            ),
            Self::NotABpm(value) => write!(
                f,
                "\"{value}\" is not a BPM. Use a number between 1 and 999, such as 124 or 127.5."
            ),
            Self::NotAYear(value) => write!(f, "\"{value}\" is not a year. Use four digits."),
        }
    }
}

impl std::error::Error for TagValueError {}

const MAX_BPM: f64 = 999.0;

/// The one form a value is stored in. Keys become Camelot codes, so "Am", "8A" and "1m" are the
/// same value. An empty value stays empty: the user blanking a field the file fills.
pub fn normalise_value(field: TagField, raw: &str) -> Result<String, TagValueError> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    match field {
        TagField::Key => value
            .parse::<Key>()
            .map(Key::camelot)
            .map_err(|_| TagValueError::NotAKey(value.to_owned())),
        TagField::Bpm => value
            .replace(',', ".")
            .parse::<f64>()
            .ok()
            .filter(|bpm| *bpm > 0.0 && *bpm <= MAX_BPM)
            .map(|bpm| bpm.to_string())
            .ok_or_else(|| TagValueError::NotABpm(value.to_owned())),
        TagField::Year => value
            .get(..4)
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_digit()))
            .map(str::to_owned)
            .ok_or_else(|| TagValueError::NotAYear(value.to_owned())),
        TagField::TrackNumber => Ok(value
            .parse::<u32>()
            .ok()
            .filter(|&number| number > 0)
            .map_or_else(|| value.to_owned(), |number| number.to_string())),
        _ => Ok(value.to_owned()),
    }
}

/// The values a track shows: each user value in place of the file's. A blank user value hides
/// the file's.
pub fn effective_tags(
    file: &[(TagField, String)],
    user: &[(TagField, String)],
) -> Vec<(TagField, String)> {
    let user_holds = |field: TagField| user.iter().any(|(held, _)| *held == field);
    let mut tags: Vec<(TagField, String)> = file
        .iter()
        .filter(|(field, _)| !user_holds(*field))
        .cloned()
        .chain(user.iter().filter(|(_, value)| !value.is_empty()).cloned())
        .collect();
    tags.sort();
    tags
}

/// One proposal per field the best candidate fills, in stored form.
pub fn proposals_from(best: &RankedCandidate) -> Vec<Proposal> {
    let candidate = &best.candidate;
    let year = candidate.year.map(|year| year.to_string());
    [
        (TagField::Artist, candidate.artist.as_deref()),
        (TagField::Title, candidate.title.as_deref()),
        (TagField::Mix, candidate.mix.as_deref()),
        (TagField::Album, candidate.album.as_deref()),
        (TagField::Label, candidate.label.as_deref()),
        (TagField::CatNo, candidate.catno.as_deref()),
        (TagField::Year, year.as_deref()),
    ]
    .into_iter()
    .filter_map(|(field, value)| {
        let value = normalise_value(field, value?).ok()?;
        (!value.is_empty()).then(|| Proposal {
            field,
            value,
            provider: candidate.provider.clone(),
            confidence: best.confidence,
            audio_matched: best.audio_matched,
        })
    })
    .collect()
}

/// The proposals that would change what the track shows. One that matches is nothing to
/// decide, so it is not staged.
pub fn changing_proposals(proposals: Vec<Proposal>, shown: &[(TagField, String)]) -> Vec<Proposal> {
    proposals
        .into_iter()
        .filter(|proposal| {
            !shown
                .iter()
                .any(|(field, value)| *field == proposal.field && *value == proposal.value)
        })
        .collect()
}

/// One field of the staging diff.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDiff {
    pub field: TagField,
    pub current_value: Option<String>,
    pub current_provenance: Option<Provenance>,
    pub proposal: Option<StoredProposal>,
    /// The user typed or accepted this field. Identification never changes it, and a proposal
    /// for it is only taken when the user accepts that one field.
    pub locked_by_user: bool,
}

impl FieldDiff {
    /// A pending proposal that would change what the track shows.
    pub fn changes(&self) -> bool {
        self.proposal.as_ref().is_some_and(|stored| {
            stored.status == ProposalStatus::Pending
                && self.current_value.as_deref() != Some(stored.proposal.value.as_str())
        })
    }
}

/// Every staged field of one track: what it shows now, where that came from, and what is
/// proposed.
pub fn staging_diff(
    file: &[(TagField, String)],
    user: &[(TagField, String)],
    proposals: &[StoredProposal],
) -> Vec<FieldDiff> {
    let find = |tags: &[(TagField, String)], field: TagField| {
        tags.iter()
            .find(|(held, _)| *held == field)
            .map(|(_, value)| value.clone())
    };
    STAGED_FIELDS
        .into_iter()
        .map(|field| {
            let user_value = find(user, field);
            let (current_value, current_provenance) = match user_value {
                Some(value) => ((!value.is_empty()).then_some(value), Some(Provenance::User)),
                None => match find(file, field).filter(|value| !value.trim().is_empty()) {
                    Some(value) => (Some(value), Some(Provenance::File)),
                    None => (None, None),
                },
            };
            FieldDiff {
                field,
                locked_by_user: current_provenance == Some(Provenance::User),
                current_value,
                current_provenance,
                proposal: proposals
                    .iter()
                    .find(|stored| stored.proposal.field == field)
                    .cloned(),
            }
        })
        .collect()
}

/// The fields "accept everything above" takes: pending, at or above `min_confidence`, a change,
/// and not held by the user.
pub fn fields_to_accept_above(diff: &[FieldDiff], min_confidence: f64) -> Vec<TagField> {
    diff.iter()
        .filter(|field| {
            !field.locked_by_user
                && field.changes()
                && field
                    .proposal
                    .as_ref()
                    .is_some_and(|stored| stored.proposal.confidence >= min_confidence)
        })
        .map(|field| field.field)
        .collect()
}

/// One field of a file that writing the user's values would change. `None` is no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldWrite {
    pub field: TagField,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// What writing `unwritten` user values into a file holding `file` would change, field by
/// field. An empty user value removes the file's value. Fields with no frame in any format are
/// left out, and so is a value the file already holds.
pub fn tag_writes(
    file: &[(TagField, String)],
    unwritten: &[(TagField, String)],
) -> Vec<FieldWrite> {
    let mut writes: Vec<FieldWrite> = unwritten
        .iter()
        .filter(|(field, _)| field.is_written_to_files())
        .map(|(field, value)| FieldWrite {
            field: *field,
            from: file
                .iter()
                .find(|(held, _)| held == field)
                .map(|(_, value)| value.clone()),
            to: (!value.is_empty()).then(|| value.clone()),
        })
        .filter(|write| write.from != write.to)
        .collect();
    writes.sort_by_key(|write| write.field);
    writes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::Candidate;

    #[test]
    fn tag_writes_skip_library_only_fields_and_values_already_in_the_file() {
        let file = tags(&[(TagField::Title, "Old"), (TagField::Genre, "House")]);
        let unwritten = tags(&[
            (TagField::Title, "New"),
            (TagField::Genre, "House"),
            (TagField::Mix, "Extended Mix"),
            (TagField::Label, ""),
            (TagField::Comment, ""),
            (TagField::Bpm, "124"),
        ]);
        assert_eq!(
            tag_writes(&file, &unwritten),
            vec![
                FieldWrite {
                    field: TagField::Title,
                    from: Some("Old".to_owned()),
                    to: Some("New".to_owned()),
                },
                FieldWrite {
                    field: TagField::Bpm,
                    from: None,
                    to: Some("124".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn an_empty_user_value_removes_the_file_value() {
        let file = tags(&[(TagField::Comment, "Ripped by someone")]);
        let unwritten = tags(&[(TagField::Comment, "")]);
        assert_eq!(
            tag_writes(&file, &unwritten),
            vec![FieldWrite {
                field: TagField::Comment,
                from: Some("Ripped by someone".to_owned()),
                to: None,
            }]
        );
    }

    fn tags(pairs: &[(TagField, &str)]) -> Vec<(TagField, String)> {
        pairs
            .iter()
            .map(|(field, value)| (*field, (*value).to_owned()))
            .collect()
    }

    fn pending(field: TagField, value: &str, confidence: f64) -> StoredProposal {
        StoredProposal {
            proposal: Proposal {
                field,
                value: value.to_owned(),
                provider: "musicbrainz".to_owned(),
                confidence,
                audio_matched: true,
            },
            status: ProposalStatus::Pending,
        }
    }

    fn diff_of(diff: &[FieldDiff], field: TagField) -> &FieldDiff {
        diff.iter().find(|entry| entry.field == field).unwrap()
    }

    #[test]
    fn keys_in_any_notation_are_stored_as_one_camelot_code() {
        for raw in ["Am", "8A", "1m", " 08a ", "A minor"] {
            assert_eq!(normalise_value(TagField::Key, raw).unwrap(), "8A", "{raw}");
        }
        assert_eq!(
            normalise_value(TagField::Key, "Q"),
            Err(TagValueError::NotAKey("Q".to_owned()))
        );
    }

    #[test]
    fn bpm_and_year_are_checked_and_other_fields_are_trimmed() {
        assert_eq!(normalise_value(TagField::Bpm, "127,50").unwrap(), "127.5");
        assert_eq!(normalise_value(TagField::Bpm, "124").unwrap(), "124");
        assert!(normalise_value(TagField::Bpm, "0").is_err());
        assert!(normalise_value(TagField::Bpm, "fast").is_err());
        assert_eq!(
            normalise_value(TagField::Year, "2019-04-01").unwrap(),
            "2019"
        );
        assert!(normalise_value(TagField::Year, "19").is_err());
        assert_eq!(normalise_value(TagField::Title, "  Song ").unwrap(), "Song");
        assert_eq!(normalise_value(TagField::Comment, "   ").unwrap(), "");
    }

    #[test]
    fn user_values_replace_file_values_and_a_blank_one_hides_it() {
        let file = tags(&[
            (TagField::Title, "File Title"),
            (TagField::Artist, "File Artist"),
            (TagField::Comment, "rip"),
        ]);
        let user = tags(&[(TagField::Artist, "Typed Artist"), (TagField::Comment, "")]);
        assert_eq!(
            effective_tags(&file, &user),
            tags(&[
                (TagField::Title, "File Title"),
                (TagField::Artist, "Typed Artist"),
            ])
        );
    }

    #[test]
    fn the_diff_shows_every_field_with_its_provenance_and_proposal() {
        let file = tags(&[
            (TagField::Title, "Song (Extended Mix)"),
            (TagField::Year, ""),
        ]);
        let user = tags(&[(TagField::Artist, "Typed Artist")]);
        let proposals = [
            pending(TagField::Title, "Song", 0.95),
            pending(TagField::Artist, "Online Artist", 0.95),
        ];
        let diff = staging_diff(&file, &user, &proposals);

        assert_eq!(
            diff.iter().map(|entry| entry.field).collect::<Vec<_>>(),
            STAGED_FIELDS
        );
        let title = diff_of(&diff, TagField::Title);
        assert_eq!(title.current_provenance, Some(Provenance::File));
        assert!(title.changes());
        assert!(!title.locked_by_user);

        let artist = diff_of(&diff, TagField::Artist);
        assert_eq!(artist.current_value.as_deref(), Some("Typed Artist"));
        assert_eq!(artist.current_provenance, Some(Provenance::User));
        assert!(artist.locked_by_user);

        let year = diff_of(&diff, TagField::Year);
        assert_eq!(year.current_value, None);
        assert_eq!(year.current_provenance, None);
    }

    #[test]
    fn accepting_above_a_threshold_skips_locked_low_unchanged_and_decided_fields() {
        let file = tags(&[(TagField::Album, "Same Album")]);
        let user = tags(&[(TagField::Artist, "Typed Artist")]);
        let mut skipped = pending(TagField::Label, "Label", 0.99);
        skipped.status = ProposalStatus::Skipped;
        let proposals = [
            pending(TagField::Title, "Song", 0.95),
            pending(TagField::Artist, "Online Artist", 0.99),
            pending(TagField::Album, "Same Album", 0.99),
            pending(TagField::Year, "2019", 0.5),
            skipped,
        ];
        let diff = staging_diff(&file, &user, &proposals);
        assert_eq!(fields_to_accept_above(&diff, 0.9), [TagField::Title]);
        assert_eq!(
            fields_to_accept_above(&diff, 0.4),
            [TagField::Title, TagField::Year]
        );
    }

    #[test]
    fn proposals_come_from_the_fields_the_best_candidate_fills() {
        let best = RankedCandidate {
            candidate: Candidate {
                provider: "discogs".to_owned(),
                artist: Some(" Artist ".to_owned()),
                title: Some("Song".to_owned()),
                mix: Some("Extended Mix".to_owned()),
                catno: Some(String::new()),
                year: Some(2019),
                ..Candidate::default()
            },
            confidence: 0.92,
            audio_matched: false,
            duration_delta_ms: None,
        };
        let proposals = proposals_from(&best);
        assert_eq!(
            proposals
                .iter()
                .map(|proposal| (proposal.field, proposal.value.as_str()))
                .collect::<Vec<_>>(),
            [
                (TagField::Artist, "Artist"),
                (TagField::Title, "Song"),
                (TagField::Mix, "Extended Mix"),
                (TagField::Year, "2019"),
            ]
        );
        assert!(proposals.iter().all(|proposal| {
            proposal.provider == "discogs" && proposal.confidence == 0.92 && !proposal.audio_matched
        }));
    }
}
