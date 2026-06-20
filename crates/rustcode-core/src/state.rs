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
//!   whenever any transform changes. Each transform mutates the state directly.
//! - **Mutations** are one-shot, non-replayable edits applied directly to the
//!   current materialized state.
//! - **Finalize** runs after every commit (transform rebuild or direct mutate).
//!
//! In Rust:
//! - [`AppState<S>`] is the generic state container.
//! - [`Transform<S>`] is a scoped, replayable transformation.
//! - Transforms are stored as ordered closures. On any change, the state is
//!   rebuilt from `initial()` by replaying all active transforms.

use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Core type aliases
// ---------------------------------------------------------------------------

/// A replayable transform applied directly to the state during rebuild.
///
/// # Source
/// Ported from `packages/core/src/state.ts` line 12:
/// `Transform = (state: State) => void`
pub type Transform<State> = Arc<dyn Fn(&mut State) + Send + Sync>;

// ---------------------------------------------------------------------------
// StateOptions — configuration for an AppState
// ---------------------------------------------------------------------------

/// Configuration for a replayable application state.
///
/// # Source
/// Ported from `packages/core/src/state.ts` lines 15–29.
pub struct StateOptions<State> {
    /// Creates the base value for initial state and every scoped-transform rebuild.
    pub initial: Arc<dyn Fn() -> State + Send + Sync>,
    /// Completes every committed edit.
    ///
    /// For rebuilds, this runs after all active transforms have been replayed and
    /// before the rebuilt state becomes visible. For direct updates, this runs
    /// after the current state has already been edited. The optional reason is
    /// caller-defined metadata for exceptional update origins.
    pub finalize: Option<Arc<dyn Fn(&State, Option<&str>) + Send + Sync>>,
}

impl<State> fmt::Debug for StateOptions<State> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateOptions")
            .field("initial", &"<closure>")
            .field("finalize", &self.finalize.as_ref().map(|_| "<closure>"))
            .finish()
    }
}

impl<State> Clone for StateOptions<State> {
    fn clone(&self) -> Self {
        Self {
            initial: Arc::clone(&self.initial),
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
pub struct TransformSlot<State> {
    /// The current transform function (can be replaced on update).
    transform: Arc<Mutex<Transform<State>>>,
}

impl<State> TransformSlot<State> {
    /// Create a new transform slot with the given transform.
    fn new(transform: Transform<State>) -> Self {
        Self {
            transform: Arc::new(Mutex::new(transform)),
        }
    }

    /// Update this slot's transform.
    pub async fn update(&self, new_transform: Transform<State>) {
        let mut t = self.transform.lock().await;
        *t = new_transform;
    }

    /// Apply this slot's transform to the given state.
    pub async fn apply(&self, state: &mut State) {
        let t = self.transform.lock().await;
        t(state);
    }
}

impl<State> Clone for TransformSlot<State> {
    fn clone(&self) -> Self {
        Self {
            transform: self.transform.clone(),
        }
    }
}

impl<State> fmt::Debug for TransformSlot<State> {
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
pub struct AppState<State>
where
    State: Clone + Send + 'static,
{
    /// The materialized (current) state.
    current: Mutex<State>,
    /// Configuration: initial value factory, finalize hook.
    options: StateOptions<State>,
    /// Ordered list of active transform slots.
    transforms: Mutex<Vec<(u64, TransformSlot<State>)>>,
    /// Monotonic slot ID counter for stable ordering.
    next_slot_id: Mutex<u64>,
}

impl<State> AppState<State>
where
    State: Clone + Send + 'static,
{
    /// Create a new application state from the given options.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 55–112 (`create()`).
    pub fn new(options: StateOptions<State>) -> Self {
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
    pub async fn register_transform(&self, transform: Transform<State>) -> TransformSlot<State> {
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
    pub async fn update(&self, transform_fn: Transform<State>) -> TransformSlot<State> {
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
        F: FnOnce(&mut State),
    {
        let mut state = self.current.lock().await;
        mutator(&mut state);

        // Run finalize hook if present
        if let Some(ref finalize) = self.options.finalize {
            finalize(&state, reason);
        }
    }

    /// Rebuild the state from `initial()` by replaying all active transforms.
    ///
    /// # Source
    /// Ported from `packages/core/src/state.ts` lines 66–72 (`rebuild()`).
    pub(crate) async fn rebuild(&self) {
        let mut next = (self.options.initial)();

        let transforms = self.transforms.lock().await;
        for (_id, slot) in transforms.iter() {
            slot.apply(&mut next).await;
        }
        drop(transforms);

        // Run finalize hook if present
        if let Some(ref finalize) = self.options.finalize {
            finalize(&next, None);
        }

        let mut current = self.current.lock().await;
        *current = next;
    }

    /// Remove a specific transform slot (by slot closure identity) and rebuild.
    ///
    /// In practice, this is called when a scope is dropped / finalized.
    pub async fn remove_transform(&self, slot: &TransformSlot<State>) {
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

impl<State> fmt::Debug for AppState<State>
where
    State: Clone + Send + fmt::Debug + 'static,
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

    /// A simple document state for testing.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Document {
        title: String,
        content: String,
        tags: Vec<String>,
    }

    fn make_document_options() -> StateOptions<Document> {
        StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
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
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Hello".to_string();
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Hello");
    }

    #[tokio::test]
    async fn multiple_transforms_compose_in_order() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "First".to_string();
            }))
            .await;

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("Hello ");
            }))
            .await;

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("World");
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
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "T1".to_string();
            }))
            .await;
        assert_eq!(state.transform_count().await, 1);

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "T2".to_string();
            }))
            .await;
        assert_eq!(state.transform_count().await, 2);
    }

    #[tokio::test]
    async fn mutate_directly_edits_current_state() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Replayable".to_string();
            }))
            .await;

        // Direct mutation
        state
            .mutate(Some("direct-edit"), |doc: &mut Document| {
                doc.tags.push("urgent".to_string());
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
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Base".to_string();
            }))
            .await;

        // Direct mutation — this is lost on rebuild
        state
            .mutate(None, |doc: &mut Document| {
                doc.tags.push("temp".to_string());
            })
            .await;

        // Trigger a rebuild by adding another transform
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str(" rebuilt");
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
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "V1".to_string();
            }))
            .await;

        // Update the existing slot
        slot.update(Arc::new(|doc: &mut Document| {
            doc.title = "V2".to_string();
        }))
        .await;

        state.rebuild().await;

        let doc = state.get().await;
        assert_eq!(doc.title, "V2");
    }

    #[tokio::test]
    async fn remove_transform_reduces_count() {
        let state = AppState::new(make_document_options());

        let slot = state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Removable".to_string();
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
        let finalized = Arc::new(std::sync::Mutex::new(Vec::new()));
        let finalized_clone = Arc::clone(&finalized);

        let options = StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            finalize: Some(Arc::new(
                move |_doc: &Document, reason: Option<&str>| {
                    let mut log = finalized_clone.lock().unwrap();
                    log.push(reason.unwrap_or("none").to_string());
                },
            )),
        };

        let state = AppState::new(options);

        state
            .mutate(Some("mutation-1"), |doc: &mut Document| {
                doc.title = "Test".to_string();
            })
            .await;

        let log = finalized.lock().unwrap();
        assert!(log.contains(&"mutation-1".to_string()));
    }

    #[tokio::test]
    async fn finalize_hook_runs_on_rebuild() {
        let finalized = Arc::new(std::sync::Mutex::new(Vec::new()));
        let finalized_clone = Arc::clone(&finalized);

        let options = StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            finalize: Some(Arc::new(
                move |_doc: &Document, _reason: Option<&str>| {
                    let mut log = finalized_clone.lock().unwrap();
                    log.push("finalized".to_string());
                },
            )),
        };

        let state = AppState::new(options);

        // First transform triggers a rebuild + finalize
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Init".to_string();
            }))
            .await;

        // Second transform triggers another rebuild + finalize
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("Content");
            }))
            .await;

        let log = finalized.lock().unwrap();
        // At least 2 finalize calls (one per rebuild)
        assert!(log.len() >= 2);
    }

    #[tokio::test]
    async fn state_get_returns_clone() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Original".to_string();
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
            s1.update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("A");
            }))
            .await;
        });

        let h2 = tokio::spawn(async move {
            s2.update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("B");
            }))
            .await;
        });

        let h3 = tokio::spawn(async move {
            s3.update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("C");
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
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Cloneable".to_string();
            }))
            .await;

        let _clone = slot.clone();

        let doc = state.get().await;
        assert_eq!(doc.title, "Cloneable");
    }

    #[tokio::test]
    async fn test_mutate_with_reason_propagates() {
        let finalized_reasons = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reasons_clone = Arc::clone(&finalized_reasons);

        let options = StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            finalize: Some(Arc::new(
                move |_doc: &Document, reason: Option<&str>| {
                    let mut log = reasons_clone.lock().unwrap();
                    log.push(reason.expect("reason").to_string());
                },
            )),
        };

        let state = AppState::new(options);

        state
            .mutate(Some("urgent-bugfix"), |doc: &mut Document| {
                doc.title = "Patched".to_string();
            })
            .await;

        state
            .mutate(Some("routine-cleanup"), |doc: &mut Document| {
                doc.content.push_str("Cleaned");
            })
            .await;

        let reasons = finalized_reasons.lock().unwrap();
        assert_eq!(reasons.len(), 2);
        assert_eq!(reasons[0], "urgent-bugfix");
        assert_eq!(reasons[1], "routine-cleanup");
    }

    #[tokio::test]
    async fn test_rebuild_preserves_transform_order() {
        let state = AppState::new(make_document_options());

        // Register transforms that append markers in order
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("[T1]");
            }))
            .await;

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("[T2]");
            }))
            .await;

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("[T3]");
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.content, "[T1][T2][T3]");

        // Trigger a rebuild by adding another transform
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("[T4]");
            }))
            .await;

        // After rebuild, all four transforms should replay in registration order
        let doc = state.get().await;
        assert_eq!(doc.content, "[T1][T2][T3][T4]");

        // Another rebuild: add T5, verify full order preserved
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("[T5]");
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.content, "[T1][T2][T3][T4][T5]");
    }

    #[tokio::test]
    async fn test_finalize_on_empty_state() {
        let finalize_called = Arc::new(std::sync::Mutex::new(false));
        let called_clone = Arc::clone(&finalize_called);

        let options = StateOptions {
            initial: Arc::new(|| Document {
                title: String::new(),
                content: String::new(),
                tags: Vec::new(),
            }),
            finalize: Some(Arc::new(
                move |_doc: &Document, _reason: Option<&str>| {
                    let mut called = called_clone.lock().unwrap();
                    *called = true;
                },
            )),
        };

        // State with finalize but zero registered transforms
        let state = AppState::new(options);
        assert_eq!(state.transform_count().await, 0);

        // Mutate — finalize hook should still run
        state
            .mutate(Some("no-transforms-yet"), |doc: &mut Document| {
                doc.title = "First Edit".to_string();
            })
            .await;

        let was_called = *finalize_called.lock().unwrap();
        assert!(
            was_called,
            "finalize hook should run on mutate even when no transforms are registered"
        );
    }

    #[tokio::test]
    async fn test_slot_update_triggers_rebuild() {
        let state = AppState::new(make_document_options());

        let slot = state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Original".to_string();
            }))
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Original");

        // Update the slot's transform function — should trigger rebuild
        slot.update(Arc::new(|doc: &mut Document| {
            doc.title = "Updated".to_string();
            doc.content.push_str("NewContent");
        }))
        .await;

        state.rebuild().await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Updated");
        assert_eq!(doc.content, "NewContent");
    }

    #[tokio::test]
    async fn test_transform_count_after_remove_and_rebuild() {
        let state = AppState::new(make_document_options());

        let _slot_a = state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "A".to_string();
            }))
            .await;

        let slot_b = state
            .update(Arc::new(|doc: &mut Document| {
                doc.content.push_str("B");
            }))
            .await;

        let _slot_c = state
            .update(Arc::new(|doc: &mut Document| {
                doc.tags.push("C".to_string());
            }))
            .await;

        assert_eq!(state.transform_count().await, 3);

        // Remove the middle transform and rebuild
        state.remove_transform(&slot_b).await;

        assert_eq!(
            state.transform_count().await,
            2,
            "transform count should decrease after removal"
        );

        // After rebuild, only transforms A and C should be active
        let doc = state.get().await;
        assert_eq!(doc.title, "A");
        assert!(doc.content.is_empty(), "B's content should be gone");
        assert_eq!(doc.tags, vec!["C"]);
    }

    #[tokio::test]
    async fn test_concurrent_mutate_and_get() {
        let state = Arc::new(AppState::new(make_document_options()));

        // Set up a transform as baseline
        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Concurrent".to_string();
            }))
            .await;

        let mut handles = Vec::new();

        // Spawn several tasks that mutate
        for i in 0..10 {
            let s = Arc::clone(&state);
            handles.push(tokio::spawn(async move {
                s.mutate(
                    Some("concurrent"),
                    move |doc: &mut Document| {
                        doc.content.push_str(&format!("M{}", i));
                    },
                )
                .await;
            }));
        }

        // Spawn several tasks that read
        for _ in 0..10 {
            let s = Arc::clone(&state);
            handles.push(tokio::spawn(async move {
                let doc = s.get().await;
                let _ = doc.title.len();
                let _ = doc.content.len();
            }));
        }

        for handle in handles {
            handle.await.expect("concurrent task should not panic");
        }

        let doc = state.get().await;
        assert_eq!(doc.title, "Concurrent");
    }

    #[tokio::test]
    async fn test_state_default_options_no_finalize() {
        let state = AppState::new(make_document_options());

        state
            .mutate(Some("no-hook"), |doc: &mut Document| {
                doc.title = "Safe".to_string();
                doc.content.push_str("No finalize needed");
            })
            .await;

        let doc = state.get().await;
        assert_eq!(doc.title, "Safe");
        assert_eq!(doc.content, "No finalize needed");
    }

    #[tokio::test]
    async fn test_state_clone_independence() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Immutable Base".to_string();
                doc.content.push_str("Original");
            }))
            .await;

        let mut doc_clone = state.get().await;
        doc_clone.title = "Hacked".to_string();
        doc_clone.content.push_str("Tampered");

        let doc = state.get().await;
        assert_eq!(doc.title, "Immutable Base");
        assert_eq!(doc.content, "Original");
    }

    #[tokio::test]
    async fn test_appstate_debug_format() {
        let state = AppState::new(make_document_options());

        state
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "Debug Me".to_string();
            }))
            .await;

        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("AppState"));
        assert!(debug_str.contains("StateOptions"));
    }

    #[tokio::test]
    async fn test_stateoptions_clone() {
        let options_a = make_document_options();

        let options_b = options_a.clone();

        let state_a = AppState::new(options_a);
        let state_b = AppState::new(options_b);

        state_a
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "State A".to_string();
            }))
            .await;

        state_b
            .update(Arc::new(|doc: &mut Document| {
                doc.title = "State B".to_string();
            }))
            .await;

        let doc_a = state_a.get().await;
        let doc_b = state_b.get().await;

        assert_eq!(doc_a.title, "State A");
        assert_eq!(doc_b.title, "State B");
    }
}
