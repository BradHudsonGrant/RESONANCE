//! The quick filters bar: the filters the user pinned, then a few recommended from what they
//! filter by most.

use crate::columns::LibraryColumn;
use crate::filter::{Condition, FieldFilter, FilterTerm};

/// Recommended filters shown at most, so the bar never crowds the table.
pub const MAX_RECOMMENDED: usize = 5;
/// Pinned filters kept at most.
pub const MAX_PINNED: usize = 12;
/// How long it takes a use to count half as much. Last month's habits matter more than last
/// year's.
const HALF_LIFE_DAYS: f64 = 30.0;
const DAY_MS: f64 = 86_400_000.0;

/// How often the user has added one filter, and when they last did.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterUse {
    pub term: FilterTerm,
    pub uses: u32,
    pub last_used_ms: i64,
}

/// What a new library recommends until real use replaces it: the gaps a DJ fixes first, and the
/// files worth replacing.
pub fn starter_filters() -> Vec<FilterTerm> {
    let one = |column, condition| FieldFilter::new(column, condition).map(FilterTerm::one);
    [
        one(LibraryColumn::Genre, Condition::Missing),
        one(LibraryColumn::Artwork, Condition::Missing),
        one(
            LibraryColumn::Format,
            Condition::AnyOf(vec!["wav".into(), "flac".into(), "aiff".into()]),
        ),
        one(LibraryColumn::Bitrate, Condition::Below(320.0)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The user's pinned filters, the recommendations they removed, and whether to recommend.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickFilterBar {
    pub pinned: Vec<FilterTerm>,
    /// Keys of recommended filters the user removed, which never come back on their own.
    pub dismissed: Vec<String>,
    pub recommend: bool,
}

impl Default for QuickFilterBar {
    fn default() -> Self {
        Self {
            pinned: Vec::new(),
            dismissed: Vec::new(),
            recommend: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuickFilterChange {
    /// Pins a filter at the end, or leaves it where it is if it is pinned.
    Pin(FilterTerm),
    /// Unpins a pinned filter, or stops a recommended one coming back.
    Remove(FilterTerm),
    /// Puts a filter at a place in the bar as shown, pinned then recommended. A recommended
    /// filter that moves becomes pinned.
    Move {
        term: FilterTerm,
        to: usize,
    },
    SetRecommend(bool),
}

impl QuickFilterBar {
    /// Recommended filters ranked by use, with recent use counting for more, then the starter
    /// filters to fill the room. Pinned and removed ones are left out.
    pub fn recommended(&self, uses: &[FilterUse], now_ms: i64) -> Vec<FilterTerm> {
        if !self.recommend {
            return Vec::new();
        }
        let mut ranked: Vec<(f64, i64, &FilterUse)> = uses
            .iter()
            .filter(|used| used.uses > 0)
            .map(|used| {
                let age_days = (now_ms - used.last_used_ms).max(0) as f64 / DAY_MS;
                let weight = f64::from(used.uses) * 0.5_f64.powf(age_days / HALF_LIFE_DAYS);
                (weight, used.last_used_ms, used)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then(b.1.cmp(&a.1))
                .then_with(|| a.2.term.key().cmp(&b.2.term.key()))
        });

        let mut shown: Vec<FilterTerm> = Vec::new();
        let candidates = ranked
            .into_iter()
            .map(|(_, _, used)| used.term.clone())
            .chain(starter_filters());
        for term in candidates {
            if shown.len() == MAX_RECOMMENDED {
                break;
            }
            let key = term.key();
            let taken = self.is_pinned(&key)
                || self.dismissed.contains(&key)
                || shown.iter().any(|existing| existing.key() == key);
            if !taken {
                shown.push(term);
            }
        }
        shown
    }

    fn is_pinned(&self, key: &str) -> bool {
        self.pinned.iter().any(|term| term.key() == key)
    }

    /// Applies a change and reports whether anything changed. `recommended` is the list the
    /// user sees, which a move counts places in.
    pub fn apply(&mut self, change: QuickFilterChange, recommended: &[FilterTerm]) -> bool {
        let before = self.clone();
        match change {
            QuickFilterChange::Pin(term) => {
                let key = term.key();
                self.dismissed.retain(|dismissed| *dismissed != key);
                if !self.is_pinned(&key) && self.pinned.len() < MAX_PINNED {
                    self.pinned.push(term);
                }
            }
            QuickFilterChange::Remove(term) => {
                let key = term.key();
                if self.is_pinned(&key) {
                    self.pinned.retain(|pinned| pinned.key() != key);
                } else if recommended.iter().any(|shown| shown.key() == key)
                    && !self.dismissed.contains(&key)
                {
                    self.dismissed.push(key);
                }
            }
            QuickFilterChange::Move { term, to } => {
                let key = term.key();
                let at = self.pinned.iter().position(|pinned| pinned.key() == key);
                let term = match at {
                    Some(at) => Some(self.pinned.remove(at)),
                    None if self.pinned.len() < MAX_PINNED
                        && recommended.iter().any(|shown| shown.key() == key) =>
                    {
                        Some(term)
                    }
                    None => None,
                };
                if let Some(term) = term {
                    let to = to.min(self.pinned.len());
                    self.pinned.insert(to, term);
                }
            }
            QuickFilterChange::SetRecommend(on) => self.recommend = on,
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const DAY: i64 = 86_400_000;

    fn term(column: LibraryColumn, condition: Condition) -> FilterTerm {
        FilterTerm::one(FieldFilter::new(column, condition).unwrap())
    }

    fn used(term: FilterTerm, uses: u32, days_ago: i64) -> FilterUse {
        FilterUse {
            term,
            uses,
            last_used_ms: NOW - days_ago * DAY,
        }
    }

    fn keys(terms: &[FilterTerm]) -> Vec<String> {
        terms.iter().map(FilterTerm::key).collect()
    }

    #[test]
    fn a_new_library_recommends_the_starter_filters() {
        let bar = QuickFilterBar::default();
        assert_eq!(keys(&bar.recommended(&[], NOW)), keys(&starter_filters()));
        assert_eq!(starter_filters().len(), 4);
    }

    #[test]
    fn the_most_used_filters_come_first_and_recent_use_counts_for_more() {
        let flac = term(LibraryColumn::Format, Condition::AnyOf(vec!["flac".into()]));
        let deep = term(LibraryColumn::Genre, Condition::Is("Deep House".into()));
        let old = term(LibraryColumn::Year, Condition::Below(2000.0));
        let uses = [
            used(old.clone(), 20, 365),
            used(flac.clone(), 3, 1),
            used(deep.clone(), 5, 2),
        ];
        let shown = QuickFilterBar::default().recommended(&uses, NOW);
        assert_eq!(shown.len(), MAX_RECOMMENDED);
        assert_eq!(keys(&shown[..3]), keys(&[deep, flac, old]));
        assert_eq!(shown[3], starter_filters()[0]);
    }

    #[test]
    fn never_more_than_five_and_none_when_turned_off() {
        let uses: Vec<FilterUse> = (0..9)
            .map(|at| {
                used(
                    term(LibraryColumn::Bpm, Condition::Above(f64::from(100 + at))),
                    9 - at as u32,
                    0,
                )
            })
            .collect();
        let mut bar = QuickFilterBar::default();
        assert_eq!(bar.recommended(&uses, NOW).len(), MAX_RECOMMENDED);
        assert!(bar.apply(QuickFilterChange::SetRecommend(false), &[]));
        assert!(bar.recommended(&uses, NOW).is_empty());
    }

    #[test]
    fn pinned_and_removed_filters_are_not_recommended() {
        let mut bar = QuickFilterBar::default();
        let starters = starter_filters();
        let shown = bar.recommended(&[], NOW);
        assert!(bar.apply(QuickFilterChange::Pin(starters[0].clone()), &shown));
        let shown = bar.recommended(&[], NOW);
        assert!(bar.apply(QuickFilterChange::Remove(starters[1].clone()), &shown));
        assert_eq!(
            keys(&bar.recommended(&[], NOW)),
            keys(&starters[2..]),
            "the pinned and removed starters are gone"
        );
        assert_eq!(keys(&bar.pinned), keys(&starters[..1]));

        // Pinning a removed one brings it back.
        bar.apply(QuickFilterChange::Pin(starters[1].clone()), &[]);
        assert!(bar.dismissed.is_empty());
    }

    #[test]
    fn moving_reorders_pins_and_pins_a_recommended_filter() {
        let mut bar = QuickFilterBar::default();
        let a = term(LibraryColumn::Genre, Condition::Is("House".into()));
        let b = term(LibraryColumn::Genre, Condition::Is("Techno".into()));
        bar.apply(QuickFilterChange::Pin(a.clone()), &[]);
        bar.apply(QuickFilterChange::Pin(b.clone()), &[]);
        assert!(bar.apply(
            QuickFilterChange::Move {
                term: b.clone(),
                to: 0
            },
            &[]
        ));
        assert_eq!(keys(&bar.pinned), keys(&[b.clone(), a.clone()]));

        let shown = bar.recommended(&[], NOW);
        let starter = shown[0].clone();
        bar.apply(
            QuickFilterChange::Move {
                term: starter.clone(),
                to: 1,
            },
            &shown,
        );
        assert_eq!(keys(&bar.pinned), keys(&[b, starter, a]));

        let stranger = term(LibraryColumn::Genre, Condition::Is("Disco".into()));
        assert!(!bar.apply(
            QuickFilterChange::Move {
                term: stranger,
                to: 0
            },
            &shown
        ));
    }

    #[test]
    fn removing_a_pinned_filter_unpins_it_without_hiding_it() {
        let mut bar = QuickFilterBar::default();
        let starter = starter_filters()[0].clone();
        bar.apply(QuickFilterChange::Pin(starter.clone()), &[]);
        bar.apply(QuickFilterChange::Remove(starter.clone()), &[]);
        assert!(bar.pinned.is_empty());
        assert!(bar.dismissed.is_empty());
        assert_eq!(bar.recommended(&[], NOW)[0], starter);
    }
}
