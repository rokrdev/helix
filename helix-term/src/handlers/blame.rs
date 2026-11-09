use std::{collections::BTreeMap, mem, sync::Arc, time::Duration};

use helix_event::register_hook;
use helix_vcs::FileBlame;
use helix_view::{
    events::{DocumentDidOpen, EditorConfigDidChange},
    handlers::{BlameEvent, Handlers},
    DocumentId,
};
use tokio::{sync::Mutex, time::Instant};

use crate::job;

/// Handles showing Git Blame for documents
#[derive(Default)]
pub struct BlameHandler {
    /// Latest request per document; BTreeMap keeps opening order.
    pending: BTreeMap<DocumentId, BlameEvent>,
    /// Serializes batches so an older walk can't overwrite newer blame.
    batch_lock: Arc<Mutex<()>>,
}

impl helix_event::AsyncHook for BlameHandler {
    type Event = BlameEvent;

    fn handle_event(
        &mut self,
        event: Self::Event,
        _timeout: Option<tokio::time::Instant>,
    ) -> Option<tokio::time::Instant> {
        self.pending.insert(event.doc_id, event);
        Some(Instant::now() + Duration::from_millis(50))
    }

    fn finish_debounce(&mut self) {
        let pending = mem::take(&mut self.pending);
        let batch_lock = Arc::clone(&self.batch_lock);
        tokio::spawn(async move {
            let _batch = batch_lock.lock().await;
            // One walk at a time so a burst of opens can't flood the blocking pool.
            for BlameEvent { path, doc_id, line } in pending.into_values() {
                let blame_path = path.clone();
                let result =
                    match tokio::task::spawn_blocking(move || FileBlame::try_new(blame_path)).await
                    {
                        Ok(result) => result,
                        Err(err) => {
                            log::error!("blame task for {} failed: {err}", path.display());
                            continue;
                        }
                    };
                job::dispatch(move |editor, _| {
                    let Some(doc) = editor.document_mut(doc_id) else {
                        return;
                    };
                    // The document may have been saved under a new path meanwhile.
                    if doc.path() != Some(&path) {
                        return;
                    }
                    doc.file_blame = Some(result);
                    if !editor.config().inline_blame.auto_fetch {
                        if let Some(line) = line {
                            crate::commands::blame_line_impl(editor, doc_id, line);
                        } else {
                            editor.set_status("Blame for this file is now available")
                        }
                    }
                })
                .await;
            }
        });
    }
}

pub(super) fn register_hooks(handlers: &Handlers) {
    let tx = handlers.blame.clone();
    register_hook!(move |event: &mut DocumentDidOpen<'_>| {
        if event.editor.config().inline_blame.auto_fetch {
            helix_event::send_blocking(
                &tx,
                BlameEvent {
                    path: event.path.to_path_buf(),
                    doc_id: event.doc,
                    line: None,
                },
            );
        }
        Ok(())
    });
    let tx = handlers.blame.clone();
    register_hook!(move |event: &mut EditorConfigDidChange<'_>| {
        if event.old_config.inline_blame != event.editor.config().inline_blame {
            // The `format`, `show`, or debounce settings changed, so any cached
            // inline-blame render state may be stale.
            crate::ui::editor::reset_inline_blame_state();
        }
        let has_enabled_inline_blame = !event.old_config.inline_blame.auto_fetch
            && event.editor.config().inline_blame.auto_fetch;

        if has_enabled_inline_blame {
            // request blame for all documents, since any of them could have
            // outdated blame
            for doc in event.editor.documents() {
                if let Some(path) = doc.path() {
                    helix_event::send_blocking(
                        &tx,
                        BlameEvent {
                            path: path.to_path_buf(),
                            doc_id: doc.id(),
                            line: None,
                        },
                    );
                }
            }
        }
        Ok(())
    });
}
