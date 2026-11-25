//! The journal of changes made to files on disk. Each change is one operation that holds what it
//! did and what it takes to put things back.

/// What an operation changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    WriteTags,
    RenameFiles,
    /// Moved files to the bin.
    DeleteFiles,
    /// Deleted files where there was no bin, after the user confirmed.
    DeleteFilesForGood,
}

impl OperationKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::WriteTags => "write_tags",
            Self::RenameFiles => "rename_files",
            Self::DeleteFiles => "delete_files",
            Self::DeleteFilesForGood => "delete_files_for_good",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "write_tags" => Some(Self::WriteTags),
            "rename_files" => Some(Self::RenameFiles),
            "delete_files" => Some(Self::DeleteFiles),
            "delete_files_for_good" => Some(Self::DeleteFilesForGood),
            _ => None,
        }
    }

    /// Whether an applied operation of this kind can be put back. A file deleted for good
    /// cannot.
    pub const fn can_undo(self, state: OperationState) -> bool {
        !matches!(self, Self::DeleteFilesForGood) && state.can_undo()
    }
}

/// An operation is pending once its plan is written and before any file changes, executing
/// while it works through its steps, applied once it has finished, failed when it changed
/// nothing, undone once its inverse has run, and rolled back when it stopped part way and the
/// steps it had done were put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationState {
    Pending,
    Executing,
    Applied,
    Failed,
    Undone,
    RolledBack,
}

impl OperationState {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Executing => "executing",
            Self::Applied => "applied",
            Self::Failed => "failed",
            Self::Undone => "undone",
            Self::RolledBack => "rolled_back",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pending" => Some(Self::Pending),
            "executing" => Some(Self::Executing),
            "applied" => Some(Self::Applied),
            "failed" => Some(Self::Failed),
            "undone" => Some(Self::Undone),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }

    /// Only a finished operation can be undone. A pending one may have stopped half way.
    pub const fn can_undo(self) -> bool {
        matches!(self, Self::Applied)
    }

    /// An operation the app stopped in the middle of, which has to be recovered when the
    /// library opens again.
    pub const fn is_unfinished(self) -> bool {
        matches!(self, Self::Pending | Self::Executing)
    }
}

/// Where one step of an operation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepState {
    Planned,
    Done,
    Undone,
}

impl StepState {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Done => "done",
            Self::Undone => "undone",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "planned" => Some(Self::Planned),
            "done" => Some(Self::Done),
            "undone" => Some(Self::Undone),
            _ => None,
        }
    }
}

/// How an unfinished operation is brought to rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    /// Every step had already happened, so the operation is recorded as applied.
    Finish,
    /// The steps that happened are put back, newest first, by position.
    RollBack(Vec<usize>),
}

/// Decides how to recover an operation from whether each of its steps took effect, as the
/// journal recorded it or as the disk shows it. A half-applied operation is never left in
/// place: either all of it stands or none of it does.
pub fn recovery(took_effect: &[bool]) -> Recovery {
    if took_effect.iter().all(|done| *done) {
        return Recovery::Finish;
    }
    Recovery::RollBack(
        took_effect
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(position, done)| done.then_some(position))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for state in [
            OperationState::Pending,
            OperationState::Executing,
            OperationState::Applied,
            OperationState::Failed,
            OperationState::Undone,
            OperationState::RolledBack,
        ] {
            assert_eq!(OperationState::from_name(state.name()), Some(state));
        }
        for kind in [
            OperationKind::WriteTags,
            OperationKind::RenameFiles,
            OperationKind::DeleteFiles,
            OperationKind::DeleteFilesForGood,
        ] {
            assert_eq!(OperationKind::from_name(kind.name()), Some(kind));
        }
        for step in [StepState::Planned, StepState::Done, StepState::Undone] {
            assert_eq!(StepState::from_name(step.name()), Some(step));
        }
        assert_eq!(OperationState::from_name("done"), None);
    }

    #[test]
    fn only_an_applied_operation_can_be_undone() {
        assert!(OperationState::Applied.can_undo());
        assert!(!OperationState::Pending.can_undo());
        assert!(!OperationState::Executing.can_undo());
        assert!(!OperationState::Failed.can_undo());
        assert!(!OperationState::Undone.can_undo());
        assert!(!OperationState::RolledBack.can_undo());
        assert!(OperationKind::DeleteFiles.can_undo(OperationState::Applied));
        assert!(!OperationKind::DeleteFilesForGood.can_undo(OperationState::Applied));
    }

    #[test]
    fn only_pending_and_executing_operations_need_recovery() {
        assert!(OperationState::Pending.is_unfinished());
        assert!(OperationState::Executing.is_unfinished());
        assert!(!OperationState::Applied.is_unfinished());
        assert!(!OperationState::RolledBack.is_unfinished());
    }

    #[test]
    fn an_operation_whose_every_step_happened_is_finished() {
        assert_eq!(recovery(&[true, true, true]), Recovery::Finish);
        assert_eq!(recovery(&[]), Recovery::Finish);
    }

    #[test]
    fn a_part_done_operation_rolls_back_newest_first() {
        assert_eq!(
            recovery(&[true, true, false]),
            Recovery::RollBack(vec![1, 0])
        );
        assert_eq!(recovery(&[false, false]), Recovery::RollBack(vec![]));
        // A step the disk shows done after one that did not happen is still put back.
        assert_eq!(
            recovery(&[true, false, true]),
            Recovery::RollBack(vec![2, 0])
        );
    }
}
