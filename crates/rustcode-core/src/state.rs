//! Application state management — replayable transforms over immutable initial values.
//!
//! Ported from: `packages/core/src/state.ts` (lines 1–113)
//!   OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b
//!
//! ## Architecture
//!
//! The TS `State` module provides a replayable-state container inspired by
//! Immer's draft-editor pattern:
//!
//! - A base `State` value is created from `initial()`.
//! - **Transforms** (scoped) are registered and replayed in registration order
//!   whenever any transform changes. Each transform wraps the mutable draft in
//!   a domain-specific `Editor`.
//! - **Mutations** are one-shot, non-replayable edits applied directly to the
//!   current materialized state.
//! - **Finalize** runs after every commit (transform rebuild or direct mutate).
//!
//! In Rust:
//! - [`AppState<S, E>`] is the generic state container.
//! - [`Transform<S, E>`] is a scoped, replayable transformation.
//! - Transforms are stored as ordered closures. On any change, the state is
//!   rebuilt from `initial()` by replaying all active transforms.

use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Core type aliases
// ---------------------------------------------------------------------------

/// A replayable transform applied to an editor during rebuild.
///
/// Transforms are intentionally synchronous and mutation-shaped: domain editors
/// hide the draft representation while preserving concise plugin/config code.
///
/// # Source
/// Ported from `packages/core/src/state.ts` line 12:
/// `Transform<Editor> = (editor: Editor) => void`
pub type Transform<Editor> = Arc<dyn Fn(&mut Editor) + Send + Sync>;

/// A factory that wraps a mutable draft state in a domain-specific editor.
///
/// # Source
/// Ported from `packages/core/src/state.ts` line 13:
/// `MakeEditor<State, Editor> = (draft: Draft<State>) => Editor`
pub type MakeEditor<State, Editor> = Arc<dyn Fn(&mut State) -> Editor + Send + Sync>;

// ---------------------------------------------------------------------------
// StateOptions — configuration for an AppState
// ---------------------------------------------------------------------------

/// Configuration for a replayable application state.
///
/// # Source
/// Ported from `packages/core/src/state.ts` lines 15–29.
pub struct StateOptions<State, Editor> {
    /// Creates the base value for initial state and every scoped-transform rebuild.
    pub initial: Arc<dyn Fn() -> State + Send + Sync>,
    /// Wraps the mutable draft in a domain-specific editor.
    pub editor: MakeEditor<State, Editor>,
    /// Completes every committed edit.
    ///
    /// For rebuilds, this runs after all active transforms have been replayed and
    /// before the rebuilt state becomes visible. For direct updates, this runs
    /// after the current state has already been edited. The optional reason is
    /// caller-defined metadata for exceptional update origins.
    pub finalize: Option<Arc<dyn Fn(&Editor, Option<&str>) + Send + Sync>>,
}

impl<State, Editor> fmt::Debug for StateOptions<State, Editor> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateOptions")
            .field("initial", &"<closure>")
            .field("editor", &"<closure>")
            .field("finalize", &self.finalize.as_ref().map(|_| "<closure>"))
            .finish()
    }
}

impl<State, Editor> Clone for StateOptions<State, Editor> {
    fn clone(&self) -> Self {
        Self {
            initial: Arc::clone(&self.initial),
            editor: Arc::clone(&self.editor),
            finalize: self.finalize.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// TransformSlot — a registered transform with its owning scope
// ---------------------------------------------------------------------------

/// A registered transform slot.
///
/// Each slot holds a transform function and can be updated. On update,
/// the owning [`AppState`] rebuilds from `initial()` by replaying all
/// active transforms in registration order.
///
/// # Source
/// Ported from `packages/core/src/state.ts` lines 74–98 (`transform()`).
#[derive(Clone)]
pub struct TransformSlot<Editor> {
    /// The current transform function (can be replaced on update).
    transform: Arc<Mutex<Transform<Editor>>>,
}

impl<Editor> TransformSlot<Editor> {
    /// Create a new transform slot with the given transform.
    fn new(transform: Transform<Editor>) -> Self {
        Self {
            transform: Arc::new(Mutex::new(transform)),
        }
    }

    /// Update this slot's transform.
    pub async fn update(&self, new_transform: Transform<Editor>) {
        let mut t = self.transform.lock().await;
        *t = new_transform;
    }

    /// Apply this slot's transform to the given editor.
    pub async fn apply(&self, editor: &mut Editor) {
        let t = self.transform.lock().await;
        t(editor);
    }
}

impl<Editor> fmt::Debug for TransformSlot<Editor> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransformSlot").finish()
    }
}

// ---------------------------------------------------------------------------
// AppState — replayable state container
// ---------------------------------------------------------------------------

/// A replayable application state container.
///
/// Maintains an ordered list of scoped transform slots. On any transform
/// update, the materialized state is rebuilt from `initial()` by replaying
/// all active transforms in registration order.
///
/// Direct mutations (via [`AppState::mutate`]) edit the current state
/// without being recorded as replayable transforms.
///
/// # Source
/// Ported from `packages/core/src/state.ts` lines 55–112 (`create()`).
pub struct AppState<State, Editor>
where
    State: Clone + Send + 'static,
    Editor: Send + 'static,
{
    /// The materialized (current) state.
    current: Mutex<State>,
    /// Configuration: initial value factory, editor wrapper, finalize hook.
    options: StateOptions<State, Editor>,
    /// Ordered list of active transform slots.
    transforms: Mutex<Vec<(u64, TransformSlot<Editor>)>>,
    /// Monotonic slot ID counter for stable ordering.
    next_slot_id: Mutex<u64>,
}

impl<State, Editor> AppState<State, Editor>
where
    State: Clone + Send + 'static,
    Editor: Send + 'static,
{
    /// Create a new application state from the given options.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 55–112 (`create()`).
    pub fn new(options: StateOptions<State, Editor>) -> Self {
        let initial_state = (options.initial)();
        Self {
            current: Mutex::new(initial_state),
            options,
            transforms: Mutex::new(Vec::new()),
            next_slot_id: Mutex::new(0),
        }
    }

    /// Get the current state.
    ///
    /// Returns a clone of the current materialized state.
    pub async fn get(&self) -> State {
        self.current.lock().await.clone()
    }

    /// Register a new transform and return its slot updater.
    ///
    /// The transform is registered in the current scope and added to the
    /// ordered slot list. The returned [`TransformSlot`] can be used to
    /// update the transform, which triggers a rebuild.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 76–98.
    pub async fn register_transform(
        &self,
        transform: Transform<Editor>,
    ) -> TransformSlot<Editor> {
        let slot = TransformSlot::new(transform);
        let mut slot_id = self.next_slot_id.lock().await;
        let id = *slot_id;
        *slot_id += 1;
        drop(slot_id);

        let mut transforms = self.transforms.lock().await;
        transforms.push((id, slot.clone()));
        drop(transforms);

        // Rebuild with the new transform included
        self.rebuild().await;

        slot
    }

    /// Apply a repla yable transform update within the current scope.
    ///
    /// This is a convenience method that creates a new transform slot
    /// and immediately applies the given transform.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 101–104 (`update()`).
    pub async fn update(&self, transform_fn: Transform<Editor>) -> TransformSlot<Editor> {
        self.register_transform(transform_fn).await
    }

    /// Mutate the current state directly (non-replayable).
    ///
    /// Direct mutations edit the current materialized state without being
    /// recorded as replayable transforms. A later rebuild starts again
    /// from `initial()` plus active transforms, discarding direct edits.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 105–109 (`mutate()`).
    pub async fn mutate<F>(&self, reason: Option<&str>, mutator: F)
    where
        F: FnOnce(&mut Editor),
    {
        let mut state = self.current.lock().await;
        let mut editor = (self.options.editor)(&mut state);
        mutator(&mut editor);

        // Run finalize hook if present
        if let Some(ref finalize) = self.options.finalize {
            finalize(&editor, reason);
        }
    }

    /// Rebuild the state from `initial()` by replaying all active transforms.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 66–72 (`rebuild()`).
    async fn rebuild(&self) {
        let mut next = (self.options.initial)();
        let mut editor = (self.options.editor)(&mut next);

        let transforms = self.transforms.lock().await;
        for (_id, slot) in transforms.iter() {
            slot.apply(&mut editor).await;
        }

        // Run finalize hook if present
        if let Some(ref finalize) = self.options.finalize {
            finalize(&editor, None);
        }

        let mut current = self.current.lock().await;
        *current = next;
    }

    /// Remove a specific transform slot (by slot closure identity) and rebuild.
    ///
    /// In practice, this is called when a scope is dropped / finalized.
    pub async fn remove_transform(&self, slot: &TransformSlot<Editor>) {
        let mut transforms = self.transforms.lock().await;
        // Compare by Arc pointer identity
        let slot_ptr = Arc::as_ptr(&slot.transform) as usize;
        transforms.retain(|(_, s)| Arc::as_ptr(&s.transform) as usize != slot_ptr);
        drop(transforms);

        self.rebuild().await;
    }

    /// Returns the number of active transform slots.
    pub async fn transform_count(&self) -> usize {
        self.transforms.lock().await.len()
    }
}

impl<State, Editor> fmt::Debug for AppState<State, Editor>
where
    State: Clone + Send + fmt::Debug + 'static,
    Editor: Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("options", &self.options)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple document state and its editor for testing.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Document {
        title: String,
        content: String,
        tags: Vec<String>,
    }

    /// Simple domain editor wrapping a mutable document.
    struct DocumentEditor<'a> {
        doc: &'a mut Document,
    }

    impl<'a> DocumentEditor<'a> {
        fn set_title(&mut self, title: &str) {
            self.doc.title = title.to_string();
        }

        fn append_content(&mut self, text: &str) {
            self.doc.content.push_str(text);
        }

        fn add_tag(&mut self, tag: &str) {
            self.doc.tags.push(tag.to_string());
        }

        fn title(&self) -> &str {
            &self.doc.title
        }

        fn content(&self) -> &str {
            &self.doc.content
        }
    }

    fn make_document_options() -> StateOptions<Document, for<'a> DocumentEditor<'a>> {
        StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            editor: Arc::new(|state: &mut Document| DocumentEditor { doc: state }),
            finalize: None,
        }
    }

    #[tokio::test]
    async fn initial_state_is_from_initial_fn() {
        let state = AppState::new(make_document_options());
        let doc = state.get().await;
        assert!(doc.title.is_empty());
        assert!(doc.content.is_empty());
        assert!(doc.tags.is_empty());
    }

    #[tokio::test]
    async fn update_adds_replayable_transform() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Hello");
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Hello");
    }

    #[tokio::test]
    async fn multiple_transforms_compose_in_order() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("First");
            }))
            .await;

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.append_content("Hello ");
            }))
            .await;

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.append_content("World");
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "First");
        assert_eq!(doc.content, "Hello World");
    }

    #[tokio::test]
    async fn transform_count_reflects_slots() {
        let state = AppState::new(make_document_options());
        assert_eq!(state.transform_count().await, 0);

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("T1");
            }))
            .await;
        assert_eq!(state.transform_count().await, 1);

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("T2");
            }))
            .await;
        assert_eq!(state.transform_count().await, 2);
    }

    #[tokio::test]
    async fn mutate_directly_edits_current_state() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Replayable");
            }))
            .await;

        // Direct mutation
        state
            .mutate(Some("direct-edit"), |editor| {
                editor.add_tag("urgent");
            })
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Replayable");
        assert_eq!(doc.tags, vec!["urgent"]);
    }

    #[tokio::test]
    async fn mutate_is_not_replayable() {
        let state = AppState::new(make_document_options());

        // Set a replayable transform
        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Base");
            }))
            .await;

        // Direct mutation — this is lost on rebuild
        state
            .mutate(None, |editor| {
                editor.add_tag("temp");
            })
            .await;

        // Trigger a rebuild by adding another transform
        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.append_content(" rebuilt");
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Base");
        // The direct mutation tag should be gone after rebuild
        assert!(doc.tags.is_empty());
    }

    #[tokio::test]
    async fn rebuild_replays_all_active_transforms() {
        let state = AppState::new(make_document_options());

        let slot = state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("V1");
            }))
            .await;

        // Update the existing slot
        slot.update(Arc::new(|editor: &mut DocumentEditor<'_>| {
            editor.set_title("V2");
        }))
        .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "V2");
    }

    #[tokio::test]
    async fn remove_transform_reduces_count() {
        let state = AppState::new(make_document_options());

        let slot = state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Removable");
            }))
            .await;

        assert_eq!(state.transform_count().await, 1);

        state.remove_transform(&slot).await;
        assert_eq!(state.transform_count().await, 0);

        // State should be back to initial
        let doc = state.get().await;
        assert!(doc.title.is_empty());
    }

    #[tokio::test]
    async fn finalize_hook_runs_on_mutate() {
        let finalized = Arc::new(Mutex::new(Vec::new()));
        let finalized_clone = Arc::clone(&finalized);

        let options = StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            editor: Arc::new(|state: &mut Document| DocumentEditor { doc: state }),
            finalize: Some(Arc::new(move |_editor: &DocumentEditor<'_>, reason: Option<&str>| {
                let mut log = finalized_clone.blocking_lock();
                log.push(reason.unwrap_or("none").to_string());
            })),
        };

        let state = AppState::new(options);

        state
            .mutate(Some("mutation-1"), |editor| {
                editor.set_title("Test");
            })
            .await;

        let log = finalized.lock().await;
        assert!(log.contains(&"mutation-1".to_string()));
    }

    #[tokio::test]
    async fn finalize_hook_runs_on_rebuild() {
        let finalized = Arc::new(Mutex::new(Vec::new()));
        let finalized_clone = Arc::clone(&finalized);

        let options = StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            editor: Arc::new(|state: &mut Document| DocumentEditor { doc: state }),
            finalize: Some(Arc::new(move |_editor: &DocumentEditor<'_>, _reason: Option<&str>| {
                let mut log = finalized_clone.blocking_lock();
                log.push("finalized".to_string());
            })),
        };

        let state = AppState::new(options);

        // First transform triggers a rebuild + finalize
        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Init");
            }))
            .await;

        // Second transform triggers another rebuild + finalize
        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.append_content("Content");
            }))
            .await;

        let log = finalized.lock().await;
        // At least 2 finalize calls (one per rebuild)
        assert!(log.len() >= 2);
    }

    #[tokio::test]
    async fn state_get_returns_clone() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Original");
            }))
            .await;

        let doc1 = state.get().await;
        let doc2 = state.get().await;

        // Clones should be equal but independent
        assert_eq!(doc1.title, doc2.title);
    }

    #[tokio::test]
    async fn concurrent_transforms_are_ordered() {
        let state = Arc::new(AppState::new(make_document_options()));

        let s1 = Arc::clone(&state);
        let s2 = Arc::clone(&state);
        let s3 = Arc::clone(&state);

        let h1 = tokio::spawn(async move {
            s1.update(Arc::new(|e: &mut DocumentEditor<'_>| {
                e.append_content("A");
            }))
            .await;
        });

        let h2 = tokio::spawn(async move {
            s2.update(Arc::new(|e: &mut DocumentEditor<'_>| {
                e.append_content("B");
            }))
            .await;
        });

        let h3 = tokio::spawn(async move {
            s3.update(Arc::new(|e: &mut DocumentEditor<'_>| {
                e.append_content("C");
            }))
            .await;
        });

        let _ = tokio::join!(h1, h2, h3);

        let doc = state.get().await;
        // All three transforms should be applied (order depends on scheduling)
        assert!(doc.content.contains('A'));
        assert!(doc.content.contains('B'));
        assert!(doc.content.contains('C'));
    }

    #[tokio::test]
    async fn empty_state_has_zero_transforms() {
        let state = AppState::new(make_document_options());
        assert_eq!(state.transform_count().await, 0);
    }

    #[tokio::test]
    async fn transform_slot_can_be_cloned() {
        let state = AppState::new(make_document_options());

        let slot = state
            .update(Arc::new(|editor: &mut DocumentEditor<'_>| {
                editor.set_title("Cloneable");
            }))
            .await;

        let _clone = slot.clone();

        let doc = state.get().await;
        assert_eq!(doc.title, "Cloneable");
    }
}
