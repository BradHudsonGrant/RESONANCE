//! The waveform overviews table rows draw, as one binary body for the rows on screen. The local
//! socket serves it at `GET /waveforms?tracks=<id>,<id>,...` and the desktop at the `waveforms`
//! scheme, so overviews never travel inside the JSON of a track page.
//!
//! Numbers are little-endian. The page reads the body in `ui/src/lib/mix/frames.ts`, which has
//! to match the offsets here:
//!
//! | Offset | Type | Field |
//! | --- | --- | --- |
//! | 0 | u8 | kind, 3 |
//! | 4 | u32 | columns in each overview |
//! | 8 | u32 | tracks |
//!
//! Then, for each track asked for and in the same order, a byte that is 1 when the track has an
//! overview and 0 when it has none yet, followed by four bytes a column (peak, low, mid, high,
//! with 255 at full scale), all zero for a track without one.

use std::sync::PoisonError;

use resonance_core::waveform::{OVERVIEW_COLUMNS, Overview};

use crate::Api;
use crate::error::ApiError;

pub const OVERVIEWS: u8 = 3;
const HEADER_LENGTH: usize = 12;
/// More rows than any screen shows at once, so one request can never ask for the library.
pub const MOST_TRACKS: usize = 500;

/// The track ids in a `tracks=1,2,3` query, or `None` when it names none, too many, or
/// something that is not an id.
pub fn track_ids(query: &str) -> Option<Vec<i64>> {
    let list = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("tracks="))?;
    let ids = list
        .split(',')
        .map(|id| id.parse::<i64>().ok())
        .collect::<Option<Vec<_>>>()?;
    (!ids.is_empty() && ids.len() <= MOST_TRACKS).then_some(ids)
}

pub fn encode(overviews: &[Option<Overview>]) -> Vec<u8> {
    let mut bytes =
        Vec::with_capacity(HEADER_LENGTH + overviews.len() * (1 + Overview::BYTES));
    bytes.extend_from_slice(&[OVERVIEWS, 0, 0, 0]);
    bytes.extend_from_slice(&(OVERVIEW_COLUMNS as u32).to_le_bytes());
    bytes.extend_from_slice(&u32::try_from(overviews.len()).unwrap_or(u32::MAX).to_le_bytes());
    for overview in overviews {
        match overview {
            Some(overview) => {
                bytes.push(1);
                bytes.extend_from_slice(&overview.to_bytes());
            }
            None => {
                bytes.push(0);
                bytes.resize(bytes.len() + Overview::BYTES, 0);
            }
        }
    }
    bytes
}

impl Api {
    /// The overviews of `track_ids`, encoded as the module documents.
    pub fn waveform_overviews(&self, track_ids: &[i64]) -> Result<Vec<u8>, ApiError> {
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let overviews = library
            .waveform_overviews(track_ids)
            .map_err(ApiError::library_unreadable)?;
        Ok(encode(&overviews))
    }
}

#[cfg(test)]
mod tests {
    use resonance_core::waveform::Bin;

    use super::*;

    #[test]
    fn a_query_names_its_tracks_in_order() {
        assert_eq!(track_ids("tracks=3,1,2"), Some(vec![3, 1, 2]));
        assert_eq!(track_ids("v=1&tracks=7"), Some(vec![7]));
        assert_eq!(track_ids("tracks="), None);
        assert_eq!(track_ids("tracks=1,x"), None);
        assert_eq!(track_ids("other=1"), None);
        let too_many = vec!["1"; MOST_TRACKS + 1].join(",");
        assert_eq!(track_ids(&format!("tracks={too_many}")), None);
    }

    #[test]
    fn overviews_round_trip_through_the_body_with_gaps_kept() {
        let mut columns = [Bin::default(); OVERVIEW_COLUMNS];
        columns[0] = Bin {
            peak: 200,
            low: 90,
            mid: 40,
            high: 10,
        };
        columns[OVERVIEW_COLUMNS - 1].high = 7;
        let overview = Overview { columns };

        let body = encode(&[None, Some(overview.clone())]);

        assert_eq!(body[0], OVERVIEWS);
        assert_eq!(body[4..8], (OVERVIEW_COLUMNS as u32).to_le_bytes());
        assert_eq!(body[8..12], 2u32.to_le_bytes());
        let entry = 1 + Overview::BYTES;
        assert_eq!(body.len(), HEADER_LENGTH + 2 * entry);
        let first = &body[HEADER_LENGTH..HEADER_LENGTH + entry];
        assert_eq!(first[0], 0);
        assert!(first[1..].iter().all(|byte| *byte == 0));
        let second = &body[HEADER_LENGTH + entry..];
        assert_eq!(second[0], 1);
        assert_eq!(second[1..5], [200, 90, 40, 10]);
        assert_eq!(Overview::from_bytes(&second[1..]), Some(overview));
    }
}
