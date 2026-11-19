//! The journal of changes made to files on disk. Each change is one operation that holds what it
//! did and what it takes to put things back.

/// What an operation changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    WriteTags,
    RenameFiles,
}

impl OperationKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::WriteTags => "write_tags",
            Self::RenameFiles => "rename_files",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "write_tags" => Some(Self::WriteTags),
            "rename_files" => Some(Self::RenameFiles),
            _ => None,
        }
    }
}

/// An operation is pending while it changes files, applied once it has finished, failed when it
/// changed nothing, and undone once its inverse has run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationState {
    Pending,
    Applied,
    Failed,
    Undone,
}

impl OperationState {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::Failed => "failed",
            Self::Undone => "undone",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pending" => Some(Self::Pending),
            "applied" => Some(Self::Applied),
            "failed" => Some(Self::Failed),
            "undone" => Some(Self::Undone),
            _ => None,
        }
    }

    /// Only a finished operation can be undone. A pending one may have stopped half way.
    pub const fn can_undo(self) -> bool {
        matches!(self, Self::Applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for state in [
            OperationState::Pending,
            OperationState::Applied,
            OperationState::Failed,
            OperationState::Undone,
        ] {
            assert_eq!(OperationState::from_name(state.name()), Some(state));
        }
        for kind in [OperationKind::WriteTags, OperationKind::RenameFiles] {
            assert_eq!(OperationKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(OperationState::from_name("done"), None);
    }

    #[test]
    fn only_an_applied_operation_can_be_undone() {
        assert!(OperationState::Applied.can_undo());
        assert!(!OperationState::Pending.can_undo());
        assert!(!OperationState::Failed.can_undo());
        assert!(!OperationState::Undone.can_undo());
    }
}
