#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobTier {
    Scan = 0,
    BackgroundCpu = 1,
    Heavy = 2,
    OnDemand = 3,
}

impl JobTier {
    pub const fn from_raw(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Scan),
            1 => Some(Self::BackgroundCpu),
            2 => Some(Self::Heavy),
            3 => Some(Self::OnDemand),
            _ => None,
        }
    }

    pub const fn raw(self) -> i64 {
        self as i64
    }

    /// Background CPU and GPU jobs pause while Mix decks are playing so audio and 240 Hz
    /// waveforms never contend for CPU or disk.
    pub const fn backs_off_for_mix(self) -> bool {
        matches!(self, Self::BackgroundCpu | Self::Heavy)
    }

    pub const fn is_interactive(self) -> bool {
        matches!(self, Self::Scan | Self::OnDemand)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }

    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    pub const fn can_cancel(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }

    pub const fn can_retry(self) -> bool {
        matches!(self, Self::Failed | Self::Cancelled)
    }
}

pub const JOB_KIND_SCAN_SOURCE: &str = "scan_source";
pub const JOB_KIND_IDENTIFY_LIBRARY: &str = "identify_library";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJob {
    pub kind: String,
    pub tier: JobTier,
    pub label: String,
    pub payload_json: String,
    pub total_items: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub tier: JobTier,
    pub status: JobStatus,
    pub label: String,
    pub payload_json: String,
    pub done_items: u64,
    pub total_items: Option<u64>,
    pub error_message: Option<String>,
    pub created_ns: i64,
    pub started_ns: Option<i64>,
    pub finished_ns: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobsSnapshot {
    pub active: Vec<Job>,
    pub recent: Vec<Job>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_round_trips_and_marks_mix_backoff_lanes() {
        for (raw, tier, interactive, backoff) in [
            (0, JobTier::Scan, true, false),
            (1, JobTier::BackgroundCpu, false, true),
            (2, JobTier::Heavy, false, true),
            (3, JobTier::OnDemand, true, false),
        ] {
            assert_eq!(JobTier::from_raw(raw), Some(tier));
            assert_eq!(tier.raw(), raw);
            assert_eq!(tier.is_interactive(), interactive);
            assert_eq!(tier.backs_off_for_mix(), backoff);
        }
        assert_eq!(JobTier::from_raw(-1), None);
        assert_eq!(JobTier::from_raw(4), None);
    }

    #[test]
    fn status_round_trips_and_enforces_cancel_and_retry_states() {
        for status in [
            JobStatus::Queued,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            assert_eq!(JobStatus::from_name(status.name()), Some(status));
        }
        assert_eq!(JobStatus::from_name("unknown"), None);

        assert!(JobStatus::Queued.can_cancel());
        assert!(JobStatus::Running.can_cancel());
        assert!(!JobStatus::Completed.can_cancel());
        assert!(!JobStatus::Failed.can_cancel());
        assert!(!JobStatus::Cancelled.can_cancel());

        assert!(!JobStatus::Queued.can_retry());
        assert!(!JobStatus::Running.can_retry());
        assert!(!JobStatus::Completed.can_retry());
        assert!(JobStatus::Failed.can_retry());
        assert!(JobStatus::Cancelled.can_retry());
    }
}
