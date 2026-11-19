use std::sync::PoisonError;

use resonance_core::columns::{
    ColumnGroup, LayoutChange, LibraryColumn, MAX_COLUMN_WIDTH_PX, MIN_COLUMN_WIDTH_PX, SortColumn,
    TableLayout,
};
use resonance_core::search::{SortDirection as CoreSortDirection, TrackSortField as CoreField};
use resonance_db::Database;

use crate::Api;
use crate::command::{
    LibraryTableLayout, SortDirection, TableColumn, TableColumnGroup, TableLayoutChange,
    TableSortKey, TrackSortField,
};
use crate::error::ApiError;
use crate::staging::{tag_field, tag_name};

impl Api {
    pub(crate) fn table_layout(&self) -> Result<LibraryTableLayout, ApiError> {
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let layout = library
            .table_layout()
            .map_err(ApiError::library_unreadable)?;
        described(&library, &layout)
    }

    pub(crate) fn change_table_layout(
        &self,
        change: TableLayoutChange,
    ) -> Result<LibraryTableLayout, ApiError> {
        let change = core_change(change)?;
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut layout = library
            .table_layout()
            .map_err(ApiError::library_unreadable)?;
        if layout.apply(change) {
            library
                .save_table_layout(&layout)
                .map_err(ApiError::library_unreadable)?;
        }
        described(&library, &layout)
    }
}

fn column(id: &str) -> Result<LibraryColumn, ApiError> {
    LibraryColumn::from_id(id)
        .ok_or_else(|| ApiError::bad_request(format!("The table has no column \"{id}\".")))
}

fn core_change(change: TableLayoutChange) -> Result<LayoutChange, ApiError> {
    Ok(match change {
        TableLayoutChange::Toggle { column: id } => LayoutChange::Toggle(column(&id)?),
        TableLayoutChange::Move { column: id, to } => LayoutChange::Move {
            column: column(&id)?,
            to: to as usize,
        },
        TableLayoutChange::Resize {
            column: id,
            width_px,
        } => LayoutChange::Resize {
            column: column(&id)?,
            width_px,
        },
        TableLayoutChange::SetWrap { wrap } => LayoutChange::SetWrap(wrap),
        TableLayoutChange::Sort { column: id, add } => LayoutChange::Sort {
            column: SortColumn::from_id(&id).ok_or_else(|| {
                ApiError::bad_request(format!("The table cannot sort by \"{id}\"."))
            })?,
            add,
        },
        TableLayoutChange::Reset => LayoutChange::Reset,
    })
}

fn described(library: &Database, layout: &TableLayout) -> Result<LibraryTableLayout, ApiError> {
    let tags = library
        .tags_in_library()
        .map_err(ApiError::library_unreadable)?;
    let mut offered: Vec<LibraryColumn> = LibraryColumn::fixed().to_vec();
    offered.extend(tags.into_iter().map(LibraryColumn::Tag));
    for shown in &layout.columns {
        if !offered.contains(&shown.column) {
            offered.push(shown.column);
        }
    }
    let entry = |column: LibraryColumn| {
        let shown = layout.position(column).map(|at| layout.columns[at]);
        TableColumn {
            id: column.id(),
            label: column.label(),
            group: match column.group() {
                ColumnGroup::Track => TableColumnGroup::Track,
                ColumnGroup::Release => TableColumnGroup::Release,
                ColumnGroup::Audio => TableColumnGroup::Audio,
                ColumnGroup::File => TableColumnGroup::File,
                ColumnGroup::Tags => TableColumnGroup::Tags,
            },
            shown: shown.is_some(),
            width_px: shown.and_then(|shown| shown.width_px),
            default_width_px: column.default_width(),
            floor_width_px: column.floor_width(),
            numeric: column.is_numeric(),
            sortable: column.sort_field().is_some(),
            removable: column != LibraryColumn::Title,
        }
    };
    Ok(LibraryTableLayout {
        columns: layout
            .columns
            .iter()
            .map(|shown| entry(shown.column))
            .collect(),
        available: offered.into_iter().map(entry).collect(),
        wrap: layout.wrap,
        sort: layout
            .sort
            .iter()
            .filter_map(|key| {
                key.column.sort_field().map(|field| TableSortKey {
                    column: key.column.id(),
                    field: api_sort_field(field),
                    direction: api_direction(key.direction),
                })
            })
            .collect(),
        drop_order: layout
            .drop_order()
            .into_iter()
            .map(LibraryColumn::id)
            .collect(),
        min_width_px: MIN_COLUMN_WIDTH_PX,
        max_width_px: MAX_COLUMN_WIDTH_PX,
    })
}

pub(crate) fn core_direction(direction: SortDirection) -> CoreSortDirection {
    match direction {
        SortDirection::Asc => CoreSortDirection::Asc,
        SortDirection::Desc => CoreSortDirection::Desc,
    }
}

fn api_direction(direction: CoreSortDirection) -> SortDirection {
    match direction {
        CoreSortDirection::Asc => SortDirection::Asc,
        CoreSortDirection::Desc => SortDirection::Desc,
    }
}

pub(crate) fn core_sort_field(field: TrackSortField) -> CoreField {
    match field {
        TrackSortField::Index => CoreField::Index,
        TrackSortField::Title => CoreField::Title,
        TrackSortField::Artist => CoreField::Artist,
        TrackSortField::Bpm => CoreField::Bpm,
        TrackSortField::Key => CoreField::Key,
        TrackSortField::Duration => CoreField::Duration,
        TrackSortField::Added => CoreField::Added,
        TrackSortField::Album => CoreField::Album,
        TrackSortField::Genre => CoreField::Genre,
        TrackSortField::Label => CoreField::Label,
        TrackSortField::CatNo => CoreField::CatNo,
        TrackSortField::Year => CoreField::Year,
        TrackSortField::Comment => CoreField::Comment,
        TrackSortField::Composer => CoreField::Composer,
        TrackSortField::Remixer => CoreField::Remixer,
        TrackSortField::Mix => CoreField::Mix,
        TrackSortField::Format => CoreField::Format,
        TrackSortField::Bitrate => CoreField::Bitrate,
        TrackSortField::SampleRate => CoreField::SampleRate,
        TrackSortField::BitDepth => CoreField::BitDepth,
        TrackSortField::Channels => CoreField::Channels,
        TrackSortField::FileSize => CoreField::FileSize,
        TrackSortField::Modified => CoreField::Modified,
        TrackSortField::Path => CoreField::Path,
        TrackSortField::Tag(name) => CoreField::Tag(tag_field(name)),
    }
}

fn api_sort_field(field: CoreField) -> TrackSortField {
    match field {
        CoreField::Index => TrackSortField::Index,
        CoreField::Title => TrackSortField::Title,
        CoreField::Artist => TrackSortField::Artist,
        CoreField::Bpm => TrackSortField::Bpm,
        CoreField::Key => TrackSortField::Key,
        CoreField::Duration => TrackSortField::Duration,
        CoreField::Added => TrackSortField::Added,
        CoreField::Album => TrackSortField::Album,
        CoreField::Genre => TrackSortField::Genre,
        CoreField::Label => TrackSortField::Label,
        CoreField::CatNo => TrackSortField::CatNo,
        CoreField::Year => TrackSortField::Year,
        CoreField::Comment => TrackSortField::Comment,
        CoreField::Composer => TrackSortField::Composer,
        CoreField::Remixer => TrackSortField::Remixer,
        CoreField::Mix => TrackSortField::Mix,
        CoreField::Format => TrackSortField::Format,
        CoreField::Bitrate => TrackSortField::Bitrate,
        CoreField::SampleRate => TrackSortField::SampleRate,
        CoreField::BitDepth => TrackSortField::BitDepth,
        CoreField::Channels => TrackSortField::Channels,
        CoreField::FileSize => TrackSortField::FileSize,
        CoreField::Modified => TrackSortField::Modified,
        CoreField::Path => TrackSortField::Path,
        CoreField::Tag(field) => TrackSortField::Tag(tag_name(field)),
    }
}
