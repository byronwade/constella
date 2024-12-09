use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use tokio::sync::mpsc;
use std::path::PathBuf;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub struct FileSystemWatcher {
    watcher: RecommendedWatcher,
    debounce_duration: Duration,
    pending_changes: HashMap<PathBuf, (Instant, ChangeType)>,
}

#[derive(Debug, Clone)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
    Renamed(PathBuf), // Old path for renamed files
}

impl FileSystemWatcher {
    pub async fn new(
        tx: mpsc::Sender<Vec<(PathBuf, ChangeType)>>,
    ) -> notify::Result<Self> {
        let (event_tx, mut event_rx) = mpsc::channel(1000);
        
        // Create watcher with raw event stream
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = event_tx.blocking_send(event);
            }
        })?;

        // Start event processor
        let debounce_duration = Duration::from_millis(500);
        let mut pending_changes = HashMap::new();

        tokio::spawn(async move {
            let mut flush_timer = tokio::time::interval(debounce_duration);

            loop {
                tokio::select! {
                    Some(event) = event_rx.recv() => {
                        // Process and debounce events
                        for path in event.paths {
                            let change_type = match event.kind {
                                notify::EventKind::Create(_) => ChangeType::Created,
                                notify::EventKind::Modify(_) => ChangeType::Modified,
                                notify::EventKind::Remove(_) => ChangeType::Deleted,
                                notify::EventKind::Rename(rename_mode) => {
                                    match rename_mode {
                                        notify::event::RenameMode::From => {
                                            continue; // Wait for "To" event
                                        },
                                        notify::event::RenameMode::To => {
                                            if let Some(from) = event.attrs.renamed_from {
                                                ChangeType::Renamed(from)
                                            } else {
                                                ChangeType::Created
                                            }
                                        },
                                        _ => continue,
                                    }
                                },
                                _ => continue,
                            };

                            pending_changes.insert(path, (Instant::now(), change_type));
                        }
                    }
                    _ = flush_timer.tick() => {
                        // Flush pending changes that are old enough
                        let now = Instant::now();
                        let mut changes = Vec::new();

                        pending_changes.retain(|path, (time, change_type)| {
                            if now.duration_since(*time) >= debounce_duration {
                                changes.push((path.clone(), change_type.clone()));
                                false
                            } else {
                                true
                            }
                        });

                        if !changes.is_empty() {
                            let _ = tx.send(changes).await;
                        }
                    }
                }
            }
        });

        Ok(Self {
            watcher,
            debounce_duration,
            pending_changes: HashMap::new(),
        })
    }

    pub fn watch(&mut self, path: impl AsRef<std::path::Path>) -> notify::Result<()> {
        self.watcher.watch(path.as_ref(), RecursiveMode::Recursive)
    }
} 