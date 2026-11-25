use std::sync::PoisonError;

use resonance_core::columns::{ColumnGroup, LibraryColumn};
use resonance_core::filter::{
    Condition, ConditionKind, FieldFilter, FieldKind, FilterTerm as CoreTerm, clean_terms,
    filterable_columns, parse_command,
};
use resonance_core::key::Key;
use resonance_core::library::AudioFormat;
use resonance_core::quality::QualityKind;
use resonance_core::quick_filters::{QuickFilterBar, QuickFilterChange as CoreChange};
use resonance_core::search::FilterPill;
use resonance_db::{Database, DbError, now_unix_ms};

use crate::command::{
    DescribedFilter, FilterChoice, FilterCondition, FilterConditionName, FilterField,
    FilterFieldKind, FilterSummary, FilterTerm, QuickFilterChange, QuickFilters, SmartView,
    TableColumnGroup, TrackFilter, TrackFilterPill, TrackQuery,
};
use crate::error::ApiError;
use crate::{Api, core_track_query};

impl Api {
    pub(crate) fn filter_fields(&self) -> Vec<FilterField> {
        filterable_columns().into_iter().map(filter_field).collect()
    }

    pub(crate) fn filter_summaries(
        &self,
        query: TrackQuery,
    ) -> Result<Vec<FilterSummary>, ApiError> {
        let query = core_track_query(Some(query))?;
        let counts = if query.filters.len() > 1 {
            let library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            library
                .query_filter_counts(&query)
                .map_err(ApiError::library_unreadable)?
                .into_iter()
                .map(Some)
                .collect()
        } else {
            vec![None; query.filters.len()]
        };
        Ok(query
            .filters
            .iter()
            .zip(counts)
            .map(|(term, count)| FilterSummary {
                label: term.label(),
                key: term.key(),
                count,
            })
            .collect())
    }

    pub(crate) fn parse_filter(&self, text: &str) -> Result<DescribedFilter, ApiError> {
        let term = parse_command(text).map_err(ApiError::invalid_input)?;
        Ok(described(&term))
    }

    pub(crate) fn record_filter_use(&self, term: FilterTerm) -> Result<QuickFilters, ApiError> {
        let term = core_term(term)?;
        let mut library = self.library();
        library
            .record_filter_use(&term, now_unix_ms())
            .map_err(ApiError::library_unreadable)?;
        quick_filters(&library)
    }

    pub(crate) fn quick_filters(&self) -> Result<QuickFilters, ApiError> {
        quick_filters(&self.library())
    }

    pub(crate) fn change_quick_filters(
        &self,
        change: QuickFilterChange,
    ) -> Result<QuickFilters, ApiError> {
        let change = match change {
            QuickFilterChange::Pin { term } => CoreChange::Pin(core_term(term)?),
            QuickFilterChange::Remove { term } => CoreChange::Remove(core_term(term)?),
            QuickFilterChange::Move { term, to } => CoreChange::Move {
                term: core_term(term)?,
                to: to as usize,
            },
            QuickFilterChange::SetRecommend { on } => CoreChange::SetRecommend(on),
        };
        let mut library = self.library();
        let (mut bar, recommended) = bar_and_recommended(&library)?;
        if bar.apply(change, &recommended) {
            library
                .save_quick_filter_bar(&bar)
                .map_err(ApiError::library_unreadable)?;
        }
        quick_filters(&library)
    }

    pub(crate) fn smart_views(&self) -> Result<Vec<SmartView>, ApiError> {
        smart_views(&self.library())
    }

    pub(crate) fn save_smart_view(
        &self,
        name: &str,
        query: TrackQuery,
    ) -> Result<Vec<SmartView>, ApiError> {
        let query = core_track_query(Some(query))?;
        let mut library = self.library();
        library
            .create_smart_view(name, query.text.as_deref(), query.pill, &query.filters)
            .map_err(view_error)?;
        smart_views(&library)
    }

    pub(crate) fn rename_smart_view(
        &self,
        view_id: i64,
        name: &str,
    ) -> Result<Vec<SmartView>, ApiError> {
        let mut library = self.library();
        let found = library
            .rename_smart_view(view_id, name)
            .map_err(view_error)?;
        if !found {
            return Err(ApiError::bad_request("That smart view is no longer there."));
        }
        smart_views(&library)
    }

    pub(crate) fn delete_smart_view(&self, view_id: i64) -> Result<Vec<SmartView>, ApiError> {
        let mut library = self.library();
        library
            .delete_smart_view(view_id)
            .map_err(ApiError::library_unreadable)?;
        smart_views(&library)
    }

    fn library(&self) -> std::sync::MutexGuard<'_, Database> {
        self.core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

fn view_error(error: DbError) -> ApiError {
    match error {
        DbError::InvalidCollectionName(_) | DbError::DuplicateViewName { .. } => {
            ApiError::invalid_input(error)
        }
        other => ApiError::library_unreadable(other),
    }
}

fn bar_and_recommended(library: &Database) -> Result<(QuickFilterBar, Vec<CoreTerm>), ApiError> {
    let bar = library
        .quick_filter_bar()
        .map_err(ApiError::library_unreadable)?;
    let uses = library
        .filter_uses()
        .map_err(ApiError::library_unreadable)?;
    let recommended = bar.recommended(&uses, now_unix_ms());
    Ok((bar, recommended))
}

fn quick_filters(library: &Database) -> Result<QuickFilters, ApiError> {
    let (bar, recommended) = bar_and_recommended(library)?;
    Ok(QuickFilters {
        pinned: bar.pinned.iter().map(described).collect(),
        recommended: recommended.iter().map(described).collect(),
        recommend: bar.recommend,
    })
}

fn smart_views(library: &Database) -> Result<Vec<SmartView>, ApiError> {
    let views = library
        .smart_views()
        .map_err(ApiError::library_unreadable)?;
    Ok(views
        .into_iter()
        .map(|view| SmartView {
            id: view.id,
            name: view.name,
            query: TrackQuery {
                text: view.search,
                pill: match view.pill {
                    FilterPill::All => None,
                    FilterPill::Favourites => Some(TrackFilterPill::Favourites),
                    FilterPill::Recent => Some(TrackFilterPill::Recent),
                },
                filters: Some(view.terms.iter().map(api_term).collect()),
                ..TrackQuery::default()
            },
        })
        .collect())
}

fn described(term: &CoreTerm) -> DescribedFilter {
    DescribedFilter {
        term: api_term(term),
        label: term.label(),
        key: term.key(),
    }
}

fn filter_field(column: LibraryColumn) -> FilterField {
    let kind = column.filter_kind();
    let choices = match column {
        LibraryColumn::Format => AudioFormat::ALL
            .into_iter()
            .map(|format| FilterChoice {
                value: format.name().to_owned(),
                label: format.name().to_uppercase(),
            })
            .collect(),
        LibraryColumn::Key => Key::all()
            .map(|key| FilterChoice {
                value: key.camelot(),
                label: key.camelot(),
            })
            .collect(),
        LibraryColumn::Quality => QualityKind::ALL
            .into_iter()
            .map(|kind| FilterChoice {
                value: kind.name().to_owned(),
                label: kind.label().to_owned(),
            })
            .collect(),
        _ => Vec::new(),
    };
    FilterField {
        column: column.id(),
        label: match column {
            LibraryColumn::Tag(_) => column.label().replacen("Tag: ", "", 1),
            _ => column.label(),
        },
        noun: column.noun(),
        group: match column.group() {
            ColumnGroup::Track => TableColumnGroup::Track,
            ColumnGroup::Release => TableColumnGroup::Release,
            ColumnGroup::Audio => TableColumnGroup::Audio,
            ColumnGroup::File => TableColumnGroup::File,
            ColumnGroup::Tags => TableColumnGroup::Tags,
        },
        kind: match kind {
            FieldKind::Text => FilterFieldKind::Text,
            FieldKind::Number => FilterFieldKind::Number,
            FieldKind::Date => FilterFieldKind::Date,
            FieldKind::Choice => FilterFieldKind::Choice,
            FieldKind::Picture => FilterFieldKind::Picture,
            FieldKind::Location => FilterFieldKind::Location,
        },
        conditions: column
            .conditions()
            .into_iter()
            .map(condition_name)
            .collect(),
        choices,
    }
}

fn condition_name(kind: ConditionKind) -> FilterConditionName {
    match kind {
        ConditionKind::Contains => FilterConditionName::Contains,
        ConditionKind::Is => FilterConditionName::Is,
        ConditionKind::IsNot => FilterConditionName::IsNot,
        ConditionKind::StartsWith => FilterConditionName::StartsWith,
        ConditionKind::AnyOf => FilterConditionName::AnyOf,
        ConditionKind::NoneOf => FilterConditionName::NoneOf,
        ConditionKind::Range => FilterConditionName::Range,
        ConditionKind::Below => FilterConditionName::Below,
        ConditionKind::Above => FilterConditionName::Above,
        ConditionKind::Missing => FilterConditionName::Missing,
        ConditionKind::Present => FilterConditionName::Present,
    }
}

pub(crate) fn core_term(term: FilterTerm) -> Result<CoreTerm, ApiError> {
    let filters = term
        .any_of
        .into_iter()
        .map(core_filter)
        .collect::<Result<Vec<_>, _>>()?;
    CoreTerm::any_of(filters).map_err(ApiError::bad_request)
}

pub(crate) fn core_terms(terms: Vec<FilterTerm>) -> Result<Vec<CoreTerm>, ApiError> {
    let terms = terms
        .into_iter()
        .map(core_term)
        .collect::<Result<Vec<_>, _>>()?;
    clean_terms(terms).map_err(ApiError::bad_request)
}

fn core_filter(filter: TrackFilter) -> Result<FieldFilter, ApiError> {
    let column = LibraryColumn::from_id(&filter.column).ok_or_else(|| {
        ApiError::bad_request(format!("There is no column \"{}\".", filter.column))
    })?;
    let condition = match filter.condition {
        FilterCondition::Contains { value } => Condition::Contains(value),
        FilterCondition::Is { value } => Condition::Is(value),
        FilterCondition::IsNot { value } => Condition::IsNot(value),
        FilterCondition::StartsWith { value } => Condition::StartsWith(value),
        FilterCondition::AnyOf { values } => Condition::AnyOf(values),
        FilterCondition::NoneOf { values } => Condition::NoneOf(values),
        FilterCondition::Range { min, max } => Condition::Range { min, max },
        FilterCondition::Below { value } => Condition::Below(value),
        FilterCondition::Above { value } => Condition::Above(value),
        FilterCondition::Missing => Condition::Missing,
        FilterCondition::Present => Condition::Present,
    };
    FieldFilter::new(column, condition).map_err(ApiError::bad_request)
}

pub(crate) fn api_term(term: &CoreTerm) -> FilterTerm {
    FilterTerm {
        any_of: term
            .filters()
            .iter()
            .map(|filter| TrackFilter {
                column: filter.column.id(),
                condition: match &filter.condition {
                    Condition::Contains(value) => FilterCondition::Contains {
                        value: value.clone(),
                    },
                    Condition::Is(value) => FilterCondition::Is {
                        value: value.clone(),
                    },
                    Condition::IsNot(value) => FilterCondition::IsNot {
                        value: value.clone(),
                    },
                    Condition::StartsWith(value) => FilterCondition::StartsWith {
                        value: value.clone(),
                    },
                    Condition::AnyOf(values) => FilterCondition::AnyOf {
                        values: values.clone(),
                    },
                    Condition::NoneOf(values) => FilterCondition::NoneOf {
                        values: values.clone(),
                    },
                    Condition::Range { min, max } => FilterCondition::Range {
                        min: *min,
                        max: *max,
                    },
                    Condition::Below(value) => FilterCondition::Below { value: *value },
                    Condition::Above(value) => FilterCondition::Above { value: *value },
                    Condition::Missing => FilterCondition::Missing,
                    Condition::Present => FilterCondition::Present,
                },
            })
            .collect(),
    }
}

/// A query with only its filters, for tests that build one in code.
#[cfg(test)]
pub(crate) fn filtered(texts: &[&str]) -> TrackQuery {
    TrackQuery {
        filters: Some(
            texts
                .iter()
                .map(|text| api_term(&parse_command(text).expect("filter reads")))
                .collect(),
        ),
        ..TrackQuery::default()
    }
}
