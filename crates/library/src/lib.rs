pub mod copies;
pub mod delete;
pub mod file_tags;
pub mod folders;
pub mod identity;
pub mod journal;
pub mod quality;
pub mod relink;
pub mod rename;
pub mod scan;
pub mod watch;
pub mod write_tags;

// Tag writes, renames and their inverses may target the same file from different jobs.
static FILE_OPERATIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn file_operation() -> std::sync::MutexGuard<'static, ()> {
    FILE_OPERATIONS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
