use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use resonance_core::library::{
    AudioFormat, folder_under_source_holding, is_ignored_folder_with, is_subfolder_excluded,
};
use resonance_db::{SourceConfig, SourceId};

const DEFAULT_QUIET_WINDOW: Duration = Duration::from_millis(250);
const MAX_COALESCE_WINDOW: Duration = Duration::from_secs(2);

enum WatchSignal {
    Dirty(SourceId, PathBuf),
    Shutdown,
}

#[derive(Clone)]
struct WatchFilter {
    watch_root: PathBuf,
    canonical_root: PathBuf,
    include_subfolders: bool,
    excluded_subfolders: Vec<String>,
    ignored_folders: Vec<String>,
    own_folders: Vec<PathBuf>,
}

impl WatchFilter {
    fn relative_parts<'a>(&self, path: &'a Path) -> Option<Vec<&'a str>> {
        let stripped = path
            .strip_prefix(&self.watch_root)
            .or_else(|_| path.strip_prefix(&self.canonical_root))
            .ok()?;
        stripped
            .components()
            .map(|part| part.as_os_str().to_str())
            .collect()
    }

    fn accepts(&self, kind: EventKind, path: &Path) -> bool {
        if self.own_folders.iter().any(|own| {
            let simplified = dunce::simplified(own);
            path.starts_with(own) || path.starts_with(simplified)
        }) {
            return false;
        }
        let Some(parts) = self.relative_parts(path) else {
            return false;
        };
        if parts.is_empty() {
            return false;
        }
        let is_audio_file = AudioFormat::of(path).is_some();
        let is_new_or_renamed_dir = path.is_dir()
            && matches!(
                kind,
                EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_))
            );
        if !is_audio_file && !is_new_or_renamed_dir {
            return false;
        }

        let dir_parts = if is_audio_file {
            &parts[..parts.len() - 1]
        } else {
            &parts[..]
        };
        if !dir_parts.is_empty() && !self.include_subfolders {
            return false;
        }
        if dir_parts
            .iter()
            .any(|segment| is_ignored_folder_with(segment, &self.ignored_folders))
        {
            return false;
        }
        if !dir_parts.is_empty() {
            let rel_dir = dir_parts.join("/");
            if is_subfolder_excluded(&rel_dir, &self.excluded_subfolders) {
                return false;
            }
        }
        true
    }
}

struct ActiveWatch {
    folder: String,
    include_subfolders: bool,
    filter: Arc<Mutex<WatchFilter>>,
    _watcher: RecommendedWatcher,
}

/// Watches every enabled library source with `notify` and invokes `on_dirty` once filesystem
/// activity in an included folder settles.
pub struct SourceWatcher {
    watched: Mutex<HashMap<SourceId, ActiveWatch>>,
    tx: Sender<WatchSignal>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl SourceWatcher {
    pub fn new<F>(on_dirty: F) -> Self
    where
        F: Fn(&Path) + Send + 'static,
    {
        Self::with_quiet_window(DEFAULT_QUIET_WINDOW, on_dirty)
    }

    pub fn with_quiet_window<F>(quiet_window: Duration, on_dirty: F) -> Self
    where
        F: Fn(&Path) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("source watcher".to_owned())
            .spawn(move || run_coalescer(rx, quiet_window, on_dirty))
            .ok();
        Self {
            watched: Mutex::new(HashMap::new()),
            tx,
            worker: Mutex::new(handle),
        }
    }

    /// Synchronises active OS directory watchers with the database's current sources and ignore
    /// rules. Sources whose `watch` flag is off or whose folder cannot be opened are not watched.
    pub fn reconcile(
        &self,
        sources: &[SourceConfig],
        ignored_folders: &[String],
        own_files: &[PathBuf],
    ) {
        let mut watched = self.watched.lock().unwrap_or_else(PoisonError::into_inner);
        let desired_ids: HashSet<SourceId> = sources
            .iter()
            .filter(|source| source.watch)
            .map(|source| source.id)
            .collect();
        watched.retain(|id, _| desired_ids.contains(id));

        for source in sources.iter().filter(|source| source.watch) {
            let Ok(canonical_root) = fs::canonicalize(&source.folder) else {
                watched.remove(&source.id);
                continue;
            };
            let watch_root = dunce::simplified(&canonical_root).to_path_buf();
            let own_folders: Vec<PathBuf> = own_files
                .iter()
                .filter_map(|own| fs::canonicalize(own).ok())
                .filter_map(|own| folder_under_source_holding(&canonical_root, &own))
                .collect();

            let next_filter = WatchFilter {
                watch_root: watch_root.clone(),
                canonical_root,
                include_subfolders: source.include_subfolders,
                excluded_subfolders: source.excluded_subfolders.clone(),
                ignored_folders: ignored_folders.to_vec(),
                own_folders,
            };

            if let Some(existing) = watched.get(&source.id)
                && existing.folder == source.folder
                && existing.include_subfolders == source.include_subfolders
            {
                let mut guard = existing
                    .filter
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                *guard = next_filter;
                continue;
            }

            watched.remove(&source.id);
            let filter = Arc::new(Mutex::new(next_filter));
            let callback_filter = Arc::clone(&filter);
            let tx = self.tx.clone();
            let source_id = source.id;
            let source_path = PathBuf::from(&source.folder);

            let Ok(mut watcher) = notify::recommended_watcher(move |res: notify::Result<Event>| {
                let Ok(event) = res else {
                    return;
                };
                if matches!(event.kind, EventKind::Access(_) | EventKind::Remove(_)) {
                    return;
                }
                let accepted = {
                    let guard = callback_filter
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    event
                        .paths
                        .iter()
                        .any(|path| guard.accepts(event.kind, path))
                };
                if accepted {
                    let _ = tx.send(WatchSignal::Dirty(source_id, source_path.clone()));
                }
            }) else {
                continue;
            };

            let mode = if source.include_subfolders {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if watcher.watch(&watch_root, mode).is_ok() {
                watched.insert(
                    source.id,
                    ActiveWatch {
                        folder: source.folder.clone(),
                        include_subfolders: source.include_subfolders,
                        filter,
                        _watcher: watcher,
                    },
                );
            }
        }
    }

    pub fn watching_ids(&self) -> HashSet<SourceId> {
        self.watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .copied()
            .collect()
    }
}

impl Drop for SourceWatcher {
    fn drop(&mut self) {
        self.watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let _ = self.tx.send(WatchSignal::Shutdown);
        if let Some(handle) = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = handle.join();
        }
    }
}

fn run_coalescer<F>(rx: Receiver<WatchSignal>, quiet_window: Duration, on_dirty: F)
where
    F: Fn(&Path),
{
    while let Ok(signal) = rx.recv() {
        let mut pending: HashMap<SourceId, PathBuf> = HashMap::new();
        match signal {
            WatchSignal::Shutdown => return,
            WatchSignal::Dirty(id, path) => {
                pending.insert(id, path);
            }
        }
        let started = Instant::now();
        loop {
            let remaining_cap = MAX_COALESCE_WINDOW.saturating_sub(started.elapsed());
            if remaining_cap.is_zero() {
                break;
            }
            let wait = quiet_window.min(remaining_cap);
            match rx.recv_timeout(wait) {
                Ok(WatchSignal::Dirty(id, path)) => {
                    pending.insert(id, path);
                }
                Ok(WatchSignal::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        for folder in pending.into_values() {
            on_dirty(&folder);
        }
    }
}
