//! Which columns the library table shows, in what order and width, whether long values wrap, and
//! how the table is sorted.
//!
//! The default table is the one a new DJ sees: title, artist, BPM, key and time. Everything else
//! is one step away in the column chooser and never shown until it is picked.

use std::fmt;

use crate::library::TagField;
use crate::search::{SortDirection, TrackSortField};

/// The narrowest a column can be dragged.
pub const MIN_COLUMN_WIDTH_PX: u16 = 40;
/// The widest a column can be dragged.
pub const MAX_COLUMN_WIDTH_PX: u16 = 640;
/// A primary sort and one secondary sort. A third key changes nothing a DJ can see in a page.
pub const MAX_SORT_KEYS: usize = 2;

/// Where the chooser lists a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnGroup {
    Track,
    Release,
    Audio,
    File,
    Tags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LibraryColumn {
    Title,
    Artist,
    Bpm,
    Key,
    Time,
    Composer,
    Remixer,
    Mix,
    Comment,
    Album,
    Genre,
    Label,
    CatNo,
    Year,
    Artwork,
    Format,
    Bitrate,
    SampleRate,
    BitDepth,
    Channels,
    FileSize,
    Added,
    Modified,
    Path,
    Folder,
    /// Any tag the library reads that has no column of its own.
    Tag(TagField),
}

const FIXED: [LibraryColumn; 25] = [
    LibraryColumn::Title,
    LibraryColumn::Artist,
    LibraryColumn::Bpm,
    LibraryColumn::Key,
    LibraryColumn::Time,
    LibraryColumn::Composer,
    LibraryColumn::Remixer,
    LibraryColumn::Mix,
    LibraryColumn::Comment,
    LibraryColumn::Album,
    LibraryColumn::Genre,
    LibraryColumn::Label,
    LibraryColumn::CatNo,
    LibraryColumn::Year,
    LibraryColumn::Artwork,
    LibraryColumn::Format,
    LibraryColumn::Bitrate,
    LibraryColumn::SampleRate,
    LibraryColumn::BitDepth,
    LibraryColumn::Channels,
    LibraryColumn::FileSize,
    LibraryColumn::Added,
    LibraryColumn::Modified,
    LibraryColumn::Path,
    LibraryColumn::Folder,
];

/// The table a new library opens with.
pub const DEFAULT_COLUMNS: [LibraryColumn; 5] = [
    LibraryColumn::Title,
    LibraryColumn::Artist,
    LibraryColumn::Bpm,
    LibraryColumn::Key,
    LibraryColumn::Time,
];

const TAG_PREFIX: &str = "tag:";

impl LibraryColumn {
    /// Every column with a place of its own, in chooser order.
    pub fn fixed() -> &'static [Self] {
        &FIXED
    }

    /// The column a tag shows in: its own if it has one, else a generic tag column.
    pub fn for_tag(field: TagField) -> Self {
        match field {
            TagField::Title => Self::Title,
            TagField::Artist => Self::Artist,
            TagField::Bpm => Self::Bpm,
            TagField::Key => Self::Key,
            TagField::Composer => Self::Composer,
            TagField::Remixer => Self::Remixer,
            TagField::Mix => Self::Mix,
            TagField::Comment => Self::Comment,
            TagField::Album => Self::Album,
            TagField::Genre => Self::Genre,
            TagField::Label => Self::Label,
            TagField::CatNo => Self::CatNo,
            TagField::Year => Self::Year,
            TagField::AlbumArtist | TagField::TrackNumber | TagField::Subgenre => Self::Tag(field),
        }
    }

    /// The stable name stored in the database and sent to the UI.
    pub fn id(self) -> String {
        match self {
            Self::Tag(field) => format!("{TAG_PREFIX}{}", field.name()),
            fixed => fixed.fixed_id().to_owned(),
        }
    }

    fn fixed_id(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Bpm => "bpm",
            Self::Key => "key",
            Self::Time => "time",
            Self::Composer => "composer",
            Self::Remixer => "remixer",
            Self::Mix => "mix",
            Self::Comment => "comment",
            Self::Album => "album",
            Self::Genre => "genre",
            Self::Label => "label",
            Self::CatNo => "catno",
            Self::Year => "year",
            Self::Artwork => "artwork",
            Self::Format => "format",
            Self::Bitrate => "bitrate",
            Self::SampleRate => "sample_rate",
            Self::BitDepth => "bit_depth",
            Self::Channels => "channels",
            Self::FileSize => "file_size",
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Path => "path",
            Self::Folder => "folder",
            Self::Tag(_) => TAG_PREFIX,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        if let Some(name) = id.strip_prefix(TAG_PREFIX) {
            let field = TagField::from_name(name)?;
            return match Self::for_tag(field) {
                Self::Tag(field) => Some(Self::Tag(field)),
                _ => None,
            };
        }
        FIXED.into_iter().find(|column| column.fixed_id() == id)
    }

    pub fn label(self) -> String {
        match self {
            Self::Tag(field) => format!("Tag: {}", tag_label(field)),
            fixed => fixed.fixed_label().to_owned(),
        }
    }

    fn fixed_label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Bpm => "BPM",
            Self::Key => "Key",
            Self::Time => "Time",
            Self::Composer => "Composer",
            Self::Remixer => "Remixer",
            Self::Mix => "Mix",
            Self::Comment => "Comment",
            Self::Album => "Album",
            Self::Genre => "Genre",
            Self::Label => "Label",
            Self::CatNo => "Catalogue number",
            Self::Year => "Year",
            Self::Artwork => "Artwork",
            Self::Format => "Format",
            Self::Bitrate => "Bitrate",
            Self::SampleRate => "Sample rate",
            Self::BitDepth => "Bit depth",
            Self::Channels => "Channels",
            Self::FileSize => "File size",
            Self::Added => "Date added",
            Self::Modified => "Date modified",
            Self::Path => "Path",
            Self::Folder => "Folder",
            Self::Tag(_) => "Tag",
        }
    }

    pub fn group(self) -> ColumnGroup {
        match self {
            Self::Title
            | Self::Artist
            | Self::Bpm
            | Self::Key
            | Self::Time
            | Self::Composer
            | Self::Remixer
            | Self::Mix
            | Self::Comment => ColumnGroup::Track,
            Self::Album | Self::Genre | Self::Label | Self::CatNo | Self::Year | Self::Artwork => {
                ColumnGroup::Release
            }
            Self::Format | Self::Bitrate | Self::SampleRate | Self::BitDepth | Self::Channels => {
                ColumnGroup::Audio
            }
            Self::FileSize | Self::Added | Self::Modified | Self::Path | Self::Folder => {
                ColumnGroup::File
            }
            Self::Tag(_) => ColumnGroup::Tags,
        }
    }

    /// Title and artist share the room the other columns leave until the user drags them.
    pub fn default_width(self) -> Option<u16> {
        match self {
            Self::Title | Self::Artist => None,
            Self::Bpm => Some(58),
            Self::Key => Some(52),
            Self::Time => Some(56),
            Self::Artwork => Some(44),
            Self::Channels | Self::BitDepth => Some(64),
            Self::Format | Self::Year => Some(64),
            Self::Bitrate | Self::SampleRate | Self::FileSize => Some(84),
            Self::Added | Self::Modified => Some(96),
            Self::CatNo => Some(112),
            Self::Path => Some(280),
            Self::Comment | Self::Folder => Some(180),
            Self::Composer
            | Self::Remixer
            | Self::Mix
            | Self::Album
            | Self::Genre
            | Self::Label
            | Self::Tag(_) => Some(140),
        }
    }

    /// The width a flexible column never drops below, and the room a fixed one needs.
    pub fn floor_width(self) -> u16 {
        match self {
            Self::Title => 150,
            Self::Artist => 120,
            other => other.default_width().unwrap_or(MIN_COLUMN_WIDTH_PX),
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Bpm
                | Self::Time
                | Self::Year
                | Self::Bitrate
                | Self::SampleRate
                | Self::BitDepth
                | Self::Channels
                | Self::FileSize
                | Self::Added
                | Self::Modified
        )
    }

    /// How a click on the column's header sorts. Artwork has no order.
    pub fn sort_field(self) -> Option<TrackSortField> {
        Some(match self {
            Self::Title => TrackSortField::Title,
            Self::Artist => TrackSortField::Artist,
            Self::Bpm => TrackSortField::Bpm,
            Self::Key => TrackSortField::Key,
            Self::Time => TrackSortField::Duration,
            Self::Composer => TrackSortField::Composer,
            Self::Remixer => TrackSortField::Remixer,
            Self::Mix => TrackSortField::Mix,
            Self::Comment => TrackSortField::Comment,
            Self::Album => TrackSortField::Album,
            Self::Genre => TrackSortField::Genre,
            Self::Label => TrackSortField::Label,
            Self::CatNo => TrackSortField::CatNo,
            Self::Year => TrackSortField::Year,
            Self::Artwork => return None,
            Self::Format => TrackSortField::Format,
            Self::Bitrate => TrackSortField::Bitrate,
            Self::SampleRate => TrackSortField::SampleRate,
            Self::BitDepth => TrackSortField::BitDepth,
            Self::Channels => TrackSortField::Channels,
            Self::FileSize => TrackSortField::FileSize,
            Self::Added => TrackSortField::Added,
            Self::Modified => TrackSortField::Modified,
            // A file's folder is the start of its path, so both put a folder's files together.
            Self::Path | Self::Folder => TrackSortField::Path,
            Self::Tag(field) => TrackSortField::Tag(field),
        })
    }
}

impl fmt::Display for LibraryColumn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id())
    }
}

fn tag_label(field: TagField) -> &'static str {
    match field {
        TagField::Title => "Title",
        TagField::Artist => "Artist",
        TagField::Album => "Album",
        TagField::AlbumArtist => "Album artist",
        TagField::Genre => "Genre",
        TagField::Year => "Year",
        TagField::TrackNumber => "Track number",
        TagField::Bpm => "BPM",
        TagField::Key => "Key",
        TagField::Label => "Label",
        TagField::Comment => "Comment",
        TagField::Mix => "Mix",
        TagField::Subgenre => "Subgenre",
        TagField::CatNo => "Catalogue number",
        TagField::Composer => "Composer",
        TagField::Remixer => "Remixer",
    }
}

/// The tags that have no column of their own, so show as generic tag columns.
pub fn generic_tag_fields() -> impl Iterator<Item = TagField> {
    TagField::ALL
        .into_iter()
        .filter(|field| matches!(LibraryColumn::for_tag(*field), LibraryColumn::Tag(_)))
}

/// A generic tag's value as it sorts: a leading number is padded so "9" comes before "10".
pub fn tag_sort_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 12 {
        return Some(trimmed.to_owned());
    }
    let (number, rest) = trimmed.split_at(digits);
    let number = number.trim_start_matches('0');
    Some(format!("{number:0>12}{rest}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownColumn {
    pub column: LibraryColumn,
    /// The width the user dragged it to. `None` keeps the column's default.
    pub width_px: Option<u16>,
    /// When the user picked it: 0 for a default column, then 1, 2, 3 in the order picked.
    pub chosen: u32,
}

/// What a sort key orders by: the library's own order, or a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    /// The `#` header: the open collection's order, else by folder and file.
    Index,
    Column(LibraryColumn),
}

impl SortColumn {
    pub fn id(self) -> String {
        match self {
            Self::Index => "index".to_owned(),
            Self::Column(column) => column.id(),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        if id == "index" {
            return Some(Self::Index);
        }
        LibraryColumn::from_id(id).map(Self::Column)
    }

    pub fn sort_field(self) -> Option<TrackSortField> {
        match self {
            Self::Index => Some(TrackSortField::Index),
            Self::Column(column) => column.sort_field(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortKey {
    pub column: SortColumn,
    pub direction: SortDirection,
}

/// The library table's columns, wrap setting and sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLayout {
    pub columns: Vec<ShownColumn>,
    pub wrap: bool,
    /// Empty means library order.
    pub sort: Vec<SortKey>,
}

impl Default for TableLayout {
    fn default() -> Self {
        Self {
            columns: DEFAULT_COLUMNS
                .into_iter()
                .map(|column| ShownColumn {
                    column,
                    width_px: None,
                    chosen: 0,
                })
                .collect(),
            wrap: false,
            sort: Vec::new(),
        }
    }
}

/// One change the user makes to the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutChange {
    /// Show a hidden column at the end, or hide a shown one.
    Toggle(LibraryColumn),
    /// Put a shown column at a place among the shown columns.
    Move {
        column: LibraryColumn,
        to: usize,
    },
    /// Set a column's width, or put back its default with `None`.
    Resize {
        column: LibraryColumn,
        width_px: Option<u16>,
    },
    SetWrap(bool),
    /// A header click. `add` is a Shift click, which adds or changes the secondary sort.
    Sort {
        column: SortColumn,
        add: bool,
    },
    Reset,
}

impl TableLayout {
    /// Builds a layout from stored parts, dropping anything this build does not know and
    /// putting back the title if it is missing.
    pub fn from_stored(columns: Vec<ShownColumn>, wrap: bool, sort: Vec<SortKey>) -> Self {
        let mut layout = Self {
            columns: Vec::new(),
            wrap,
            sort: Vec::new(),
        };
        for shown in columns {
            if layout.position(shown.column).is_none() {
                layout.columns.push(ShownColumn {
                    width_px: shown.width_px.map(clamp_width),
                    ..shown
                });
            }
        }
        if layout.columns.is_empty() {
            layout.columns = Self::default().columns;
        }
        if layout.position(LibraryColumn::Title).is_none() {
            layout.columns.insert(
                0,
                ShownColumn {
                    column: LibraryColumn::Title,
                    width_px: None,
                    chosen: 0,
                },
            );
        }
        for key in sort {
            layout.push_sort(key);
        }
        layout
    }

    fn push_sort(&mut self, key: SortKey) {
        let sortable = key.column.sort_field().is_some();
        let shown = match key.column {
            SortColumn::Index => self.sort.is_empty(),
            SortColumn::Column(column) => self.position(column).is_some(),
        };
        let repeated = self
            .sort
            .iter()
            .any(|existing| existing.column == key.column);
        let after_index = self
            .sort
            .first()
            .is_some_and(|first| first.column == SortColumn::Index);
        if sortable && shown && !repeated && !after_index && self.sort.len() < MAX_SORT_KEYS {
            self.sort.push(key);
        }
    }

    pub fn position(&self, column: LibraryColumn) -> Option<usize> {
        self.columns.iter().position(|shown| shown.column == column)
    }

    /// Applies a change and reports whether anything changed.
    pub fn apply(&mut self, change: LayoutChange) -> bool {
        let before = self.clone();
        match change {
            LayoutChange::Toggle(column) => self.toggle(column),
            LayoutChange::Move { column, to } => {
                if let Some(from) = self.position(column) {
                    let shown = self.columns.remove(from);
                    let to = to.min(self.columns.len());
                    self.columns.insert(to, shown);
                }
            }
            LayoutChange::Resize { column, width_px } => {
                if let Some(at) = self.position(column) {
                    self.columns[at].width_px = width_px.map(clamp_width);
                }
            }
            LayoutChange::SetWrap(wrap) => self.wrap = wrap,
            LayoutChange::Sort { column, add } => self.sort_by(column, add),
            LayoutChange::Reset => *self = Self::default(),
        }
        *self != before
    }

    /// The title stays: a table that cannot name its tracks is no use.
    fn toggle(&mut self, column: LibraryColumn) {
        if let Some(at) = self.position(column) {
            if column != LibraryColumn::Title {
                self.columns.remove(at);
                self.sort
                    .retain(|key| key.column != SortColumn::Column(column));
            }
            return;
        }
        let chosen = if DEFAULT_COLUMNS.contains(&column) {
            0
        } else {
            self.columns
                .iter()
                .map(|shown| shown.chosen)
                .max()
                .unwrap_or(0)
                + 1
        };
        self.columns.push(ShownColumn {
            column,
            width_px: None,
            chosen,
        });
    }

    /// A click sorts by the column, then reverses it, then goes back to library order. A Shift
    /// click does the same for the secondary sort and leaves the primary alone.
    fn sort_by(&mut self, column: SortColumn, add: bool) {
        if column.sort_field().is_none() {
            return;
        }
        if let SortColumn::Column(shown) = column
            && self.position(shown).is_none()
        {
            return;
        }
        // Library order is the default, so `#` reverses it from there and anything else goes
        // back to it.
        if column == SortColumn::Index {
            self.sort = if self.sort.is_empty() {
                vec![SortKey {
                    column,
                    direction: SortDirection::Desc,
                }]
            } else {
                Vec::new()
            };
            return;
        }
        let primary_is_index = self
            .sort
            .first()
            .is_some_and(|key| key.column == SortColumn::Index);
        if primary_is_index {
            self.sort.clear();
        }
        let slot = self.sort.iter().position(|key| key.column == column);
        if !add || self.sort.is_empty() || slot == Some(0) {
            match slot {
                Some(0) if self.sort[0].direction == SortDirection::Asc => {
                    self.sort[0].direction = SortDirection::Desc;
                }
                Some(0) => {
                    self.sort.remove(0);
                    if !add {
                        self.sort.clear();
                    }
                }
                _ => {
                    self.sort = vec![SortKey {
                        column,
                        direction: SortDirection::Asc,
                    }];
                }
            }
            return;
        }
        match slot {
            Some(at) if self.sort[at].direction == SortDirection::Asc => {
                self.sort[at].direction = SortDirection::Desc;
            }
            Some(at) => {
                self.sort.remove(at);
            }
            None => {
                self.sort.truncate(MAX_SORT_KEYS - 1);
                self.sort.push(SortKey {
                    column,
                    direction: SortDirection::Asc,
                });
            }
        }
    }

    /// The order chosen columns give way when the table is too narrow for them all: the last
    /// picked goes first. Default columns never drop.
    pub fn drop_order(&self) -> Vec<LibraryColumn> {
        let mut chosen: Vec<&ShownColumn> = self
            .columns
            .iter()
            .filter(|shown| !DEFAULT_COLUMNS.contains(&shown.column))
            .collect();
        chosen.sort_by_key(|shown| std::cmp::Reverse(shown.chosen));
        chosen.into_iter().map(|shown| shown.column).collect()
    }

    /// The sort the track query runs: primary, then secondary.
    pub fn query_sort(&self) -> Vec<(TrackSortField, SortDirection)> {
        self.sort
            .iter()
            .filter_map(|key| key.column.sort_field().map(|field| (field, key.direction)))
            .collect()
    }
}

fn clamp_width(width: u16) -> u16 {
    width.clamp(MIN_COLUMN_WIDTH_PX, MAX_COLUMN_WIDTH_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(layout: &TableLayout) -> Vec<String> {
        layout
            .columns
            .iter()
            .map(|shown| shown.column.id())
            .collect()
    }

    fn sort(layout: &TableLayout) -> Vec<(String, SortDirection)> {
        layout
            .sort
            .iter()
            .map(|key| (key.column.id(), key.direction))
            .collect()
    }

    #[test]
    fn a_new_table_shows_the_five_default_columns_in_library_order() {
        let layout = TableLayout::default();
        assert_eq!(ids(&layout), ["title", "artist", "bpm", "key", "time"]);
        assert!(!layout.wrap);
        assert!(layout.query_sort().is_empty());
    }

    #[test]
    fn every_column_id_reads_back_as_the_same_column() {
        for column in LibraryColumn::fixed()
            .iter()
            .copied()
            .chain(generic_tag_fields().map(LibraryColumn::Tag))
        {
            assert_eq!(LibraryColumn::from_id(&column.id()), Some(column));
        }
        assert_eq!(LibraryColumn::from_id("tag:genre"), None);
        assert_eq!(LibraryColumn::from_id("tag:nonsense"), None);
        assert_eq!(LibraryColumn::from_id("tension"), None);
    }

    #[test]
    fn generic_tag_columns_are_the_tags_without_a_column() {
        let generic: Vec<TagField> = generic_tag_fields().collect();
        assert_eq!(
            generic,
            [
                TagField::AlbumArtist,
                TagField::TrackNumber,
                TagField::Subgenre
            ]
        );
        assert_eq!(
            LibraryColumn::Tag(TagField::AlbumArtist).label(),
            "Tag: Album artist"
        );
        assert_eq!(
            LibraryColumn::Tag(TagField::Subgenre).group(),
            ColumnGroup::Tags
        );
    }

    #[test]
    fn toggling_adds_at_the_end_and_hides_anything_but_the_title() {
        let mut layout = TableLayout::default();
        assert!(layout.apply(LayoutChange::Toggle(LibraryColumn::Genre)));
        assert!(layout.apply(LayoutChange::Toggle(LibraryColumn::Bitrate)));
        assert_eq!(
            ids(&layout),
            ["title", "artist", "bpm", "key", "time", "genre", "bitrate"]
        );
        assert_eq!(layout.columns[5].chosen, 1);
        assert_eq!(layout.columns[6].chosen, 2);

        assert!(layout.apply(LayoutChange::Toggle(LibraryColumn::Artist)));
        assert!(!layout.apply(LayoutChange::Toggle(LibraryColumn::Title)));
        assert_eq!(
            ids(&layout),
            ["title", "bpm", "key", "time", "genre", "bitrate"]
        );
    }

    #[test]
    fn hiding_a_sorted_column_drops_its_sort() {
        let mut layout = TableLayout::default();
        layout.apply(LayoutChange::Toggle(LibraryColumn::Genre));
        layout.apply(LayoutChange::Sort {
            column: SortColumn::Column(LibraryColumn::Genre),
            add: false,
        });
        layout.apply(LayoutChange::Toggle(LibraryColumn::Genre));
        assert!(layout.sort.is_empty());
    }

    #[test]
    fn moving_and_resizing_keep_widths_inside_their_bounds() {
        let mut layout = TableLayout::default();
        assert!(layout.apply(LayoutChange::Move {
            column: LibraryColumn::Time,
            to: 0,
        }));
        assert_eq!(ids(&layout), ["time", "title", "artist", "bpm", "key"]);
        layout.apply(LayoutChange::Move {
            column: LibraryColumn::Time,
            to: 99,
        });
        assert_eq!(ids(&layout), ["title", "artist", "bpm", "key", "time"]);

        layout.apply(LayoutChange::Resize {
            column: LibraryColumn::Bpm,
            width_px: Some(4),
        });
        assert_eq!(layout.columns[2].width_px, Some(MIN_COLUMN_WIDTH_PX));
        layout.apply(LayoutChange::Resize {
            column: LibraryColumn::Bpm,
            width_px: Some(5000),
        });
        assert_eq!(layout.columns[2].width_px, Some(MAX_COLUMN_WIDTH_PX));
        layout.apply(LayoutChange::Resize {
            column: LibraryColumn::Bpm,
            width_px: None,
        });
        assert_eq!(layout.columns[2].width_px, None);
    }

    #[test]
    fn a_click_sorts_up_then_down_then_back_to_library_order() {
        let mut layout = TableLayout::default();
        let bpm = SortColumn::Column(LibraryColumn::Bpm);
        layout.apply(LayoutChange::Sort {
            column: bpm,
            add: false,
        });
        assert_eq!(sort(&layout), [("bpm".to_owned(), SortDirection::Asc)]);
        layout.apply(LayoutChange::Sort {
            column: bpm,
            add: false,
        });
        assert_eq!(sort(&layout), [("bpm".to_owned(), SortDirection::Desc)]);
        layout.apply(LayoutChange::Sort {
            column: bpm,
            add: false,
        });
        assert!(layout.sort.is_empty());
    }

    #[test]
    fn shift_adds_a_secondary_sort_and_cycles_it_on_its_own() {
        let mut layout = TableLayout::default();
        let key = SortColumn::Column(LibraryColumn::Key);
        let bpm = SortColumn::Column(LibraryColumn::Bpm);
        let time = SortColumn::Column(LibraryColumn::Time);
        layout.apply(LayoutChange::Sort {
            column: key,
            add: true,
        });
        layout.apply(LayoutChange::Sort {
            column: bpm,
            add: true,
        });
        assert_eq!(
            sort(&layout),
            [
                ("key".to_owned(), SortDirection::Asc),
                ("bpm".to_owned(), SortDirection::Asc)
            ]
        );
        layout.apply(LayoutChange::Sort {
            column: bpm,
            add: true,
        });
        assert_eq!(layout.sort[1].direction, SortDirection::Desc);
        layout.apply(LayoutChange::Sort {
            column: time,
            add: true,
        });
        assert_eq!(
            sort(&layout),
            [
                ("key".to_owned(), SortDirection::Asc),
                ("time".to_owned(), SortDirection::Asc)
            ]
        );
        layout.apply(LayoutChange::Sort {
            column: time,
            add: true,
        });
        layout.apply(LayoutChange::Sort {
            column: time,
            add: true,
        });
        assert_eq!(sort(&layout), [("key".to_owned(), SortDirection::Asc)]);

        layout.apply(LayoutChange::Sort {
            column: bpm,
            add: true,
        });
        layout.apply(LayoutChange::Sort {
            column: time,
            add: false,
        });
        assert_eq!(sort(&layout), [("time".to_owned(), SortDirection::Asc)]);
    }

    #[test]
    fn the_index_header_reverses_library_order_and_takes_no_secondary() {
        let mut layout = TableLayout::default();
        layout.apply(LayoutChange::Sort {
            column: SortColumn::Index,
            add: true,
        });
        assert_eq!(sort(&layout), [("index".to_owned(), SortDirection::Desc)]);
        layout.apply(LayoutChange::Sort {
            column: SortColumn::Index,
            add: false,
        });
        assert!(layout.sort.is_empty());
        layout.apply(LayoutChange::Sort {
            column: SortColumn::Index,
            add: false,
        });
        layout.apply(LayoutChange::Sort {
            column: SortColumn::Column(LibraryColumn::Bpm),
            add: true,
        });
        assert_eq!(sort(&layout), [("bpm".to_owned(), SortDirection::Asc)]);
    }

    #[test]
    fn artwork_and_hidden_columns_do_not_sort() {
        let mut layout = TableLayout::default();
        layout.apply(LayoutChange::Toggle(LibraryColumn::Artwork));
        assert!(!layout.apply(LayoutChange::Sort {
            column: SortColumn::Column(LibraryColumn::Artwork),
            add: false,
        }));
        assert!(!layout.apply(LayoutChange::Sort {
            column: SortColumn::Column(LibraryColumn::Genre),
            add: false,
        }));
    }

    #[test]
    fn the_last_picked_column_drops_first_and_defaults_never_drop() {
        let mut layout = TableLayout::default();
        layout.apply(LayoutChange::Toggle(LibraryColumn::Genre));
        layout.apply(LayoutChange::Toggle(LibraryColumn::Label));
        layout.apply(LayoutChange::Move {
            column: LibraryColumn::Label,
            to: 1,
        });
        layout.apply(LayoutChange::Toggle(LibraryColumn::Artist));
        layout.apply(LayoutChange::Toggle(LibraryColumn::Artist));
        assert_eq!(
            layout.drop_order(),
            [LibraryColumn::Label, LibraryColumn::Genre]
        );
    }

    #[test]
    fn a_stored_layout_drops_repeats_and_keeps_the_title() {
        let shown = |column, chosen| ShownColumn {
            column,
            width_px: Some(9999),
            chosen,
        };
        let layout = TableLayout::from_stored(
            vec![
                shown(LibraryColumn::Genre, 1),
                shown(LibraryColumn::Genre, 2),
            ],
            true,
            vec![
                SortKey {
                    column: SortColumn::Column(LibraryColumn::Genre),
                    direction: SortDirection::Desc,
                },
                SortKey {
                    column: SortColumn::Column(LibraryColumn::Genre),
                    direction: SortDirection::Asc,
                },
                SortKey {
                    column: SortColumn::Column(LibraryColumn::Album),
                    direction: SortDirection::Asc,
                },
            ],
        );
        assert_eq!(ids(&layout), ["title", "genre"]);
        assert_eq!(layout.columns[1].width_px, Some(MAX_COLUMN_WIDTH_PX));
        assert!(layout.wrap);
        assert_eq!(
            layout.query_sort(),
            [(TrackSortField::Genre, SortDirection::Desc)]
        );
    }

    #[test]
    fn tag_sort_values_put_numbers_in_number_order() {
        assert!(tag_sort_value("9").unwrap() < tag_sort_value("10").unwrap());
        assert!(tag_sort_value("2/12").unwrap() < tag_sort_value("11/12").unwrap());
        assert_eq!(tag_sort_value("  "), None);
        assert_eq!(tag_sort_value(" Deep "), Some("Deep".to_owned()));
    }
}
