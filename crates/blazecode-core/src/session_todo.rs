//! Session todo types — todo items, lists, and related enums.
//!
//! Ported from:
//! - `packages/core/src/session/todo.ts` (lines 1–92)
//! - `packages/blazecode/src/session/todo.ts` (lines 1–91)

use crate::session_info::SessionId;
use serde::{Deserialize, Serialize};

// ══════════════════════════════════════════════════════════════════════════════
// Todo Info
// ══════════════════════════════════════════════════════════════════════════════

/// A single todo item.
///
/// # Source
/// `packages/core/src/session/todo.ts` lines 10–17 `Info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Brief description of the task
    pub content: String,
    /// Current status: pending, in_progress, completed, cancelled
    pub status: TodoStatus,
    /// Priority level: high, medium, low
    pub priority: TodoPriority,
}

/// Status of a todo item.
///
/// # Source
/// `packages/core/src/session/todo.ts` line 13 `status` annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    /// Not yet started
    Pending,
    /// Currently being worked on
    #[serde(rename = "in_progress")]
    InProgress,
    /// Successfully completed
    Completed,
    /// Cancelled / no longer needed
    Cancelled,
}

/// Priority level of a todo item.
///
/// # Source
/// `packages/core/src/session/todo.ts` line 15 `priority` annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TodoPriority {
    /// Highest urgency
    High,
    /// Medium urgency
    Medium,
    /// Lowest urgency
    Low,
}

// ══════════════════════════════════════════════════════════════════════════════
// Todo Update Input
// ══════════════════════════════════════════════════════════════════════════════

/// Input for updating todos on a session.
///
/// # Source
/// `packages/core/src/session/todo.ts` lines 29–34 `Interface.update` input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoUpdateInput {
    pub session_id: SessionId,
    pub todos: Vec<TodoItem>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Todo Event
// ══════════════════════════════════════════════════════════════════════════════

/// Payload for the "todo.updated" event.
///
/// # Source
/// `packages/core/src/session/todo.ts` lines 19–27 `Event.Updated`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoUpdatedEvent {
    /// Session that owns the todos
    pub session_id: SessionId,
    /// Full list of todos after update
    pub todos: Vec<TodoItem>,
}

// ══════════════════════════════════════════════════════════════════════════════
// Todo Store Interface
// ══════════════════════════════════════════════════════════════════════════════

/// Trait for todo CRUD operations.
///
/// # Source
/// `packages/core/src/session/todo.ts` lines 29–35 `Interface`.
pub trait SessionTodoStore: Send + Sync {
    /// Replace all todos for a session.
    fn update(
        &self,
        input: TodoUpdateInput,
    ) -> impl std::future::Future<Output = Result<(), TodoError>> + Send;

    /// Get all todos for a session.
    fn get(
        &self,
        session_id: SessionId,
    ) -> impl std::future::Future<Output = Result<Vec<TodoItem>, TodoError>> + Send;
}

// ══════════════════════════════════════════════════════════════════════════════
// Todo Error
// ══════════════════════════════════════════════════════════════════════════════

/// Errors from todo operations.
#[derive(Debug, Clone)]
pub struct TodoError {
    pub message: String,
}

impl std::fmt::Display for TodoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TodoError: {}", self.message)
    }
}

impl std::error::Error for TodoError {}

// ══════════════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_item_serialization() {
        let item = TodoItem {
            content: "Implement login".into(),
            status: TodoStatus::Pending,
            priority: TodoPriority::High,
        };
        let json = serde_json::to_string(&item).expect("serialize");
        assert!(json.contains("Implement login"));
        assert!(json.contains("pending"));
        assert!(json.contains("high"));
        let parsed: TodoItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.content, "Implement login");
        assert_eq!(parsed.status, TodoStatus::Pending);
        assert_eq!(parsed.priority, TodoPriority::High);
    }

    #[test]
    fn test_todo_item_in_progress() {
        let item = TodoItem {
            content: "Write tests".into(),
            status: TodoStatus::InProgress,
            priority: TodoPriority::Medium,
        };
        let json = serde_json::to_string(&item).expect("serialize");
        assert!(json.contains("in_progress"));
        let parsed: TodoItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.status, TodoStatus::InProgress);
    }

    #[test]
    fn test_todo_item_completed() {
        let item = TodoItem {
            content: "Set up CI".into(),
            status: TodoStatus::Completed,
            priority: TodoPriority::Low,
        };
        let json = serde_json::to_string(&item).expect("serialize");
        assert!(json.contains("completed"));
        let parsed: TodoItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.status, TodoStatus::Completed);
    }

    #[test]
    fn test_todo_item_cancelled() {
        let item = TodoItem {
            content: "Old task".into(),
            status: TodoStatus::Cancelled,
            priority: TodoPriority::Low,
        };
        let json = serde_json::to_string(&item).expect("serialize");
        assert!(json.contains("cancelled"));
        let parsed: TodoItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.status, TodoStatus::Cancelled);
    }

    #[test]
    fn test_todo_priority_all_variants() {
        let priorities = [TodoPriority::High, TodoPriority::Medium, TodoPriority::Low];
        let expected = ["high", "medium", "low"];
        for (pri, exp) in priorities.iter().zip(expected.iter()) {
            let json = serde_json::to_string(pri).expect("serialize");
            assert!(json.contains(exp), "expected {} for {:?}", exp, pri);
            let parsed: TodoPriority = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(*pri, parsed);
        }
    }

    #[test]
    fn test_todo_status_all_variants() {
        let statuses = [
            TodoStatus::Pending,
            TodoStatus::InProgress,
            TodoStatus::Completed,
            TodoStatus::Cancelled,
        ];
        let expected = ["pending", "in_progress", "completed", "cancelled"];
        for (st, exp) in statuses.iter().zip(expected.iter()) {
            let json = serde_json::to_string(st).expect("serialize");
            assert!(json.contains(exp), "expected {} for {:?}", exp, st);
        }
    }

    #[test]
    fn test_todo_update_input() {
        let input = TodoUpdateInput {
            session_id: "ses_001".into(),
            todos: vec![
                TodoItem {
                    content: "Task 1".into(),
                    status: TodoStatus::Pending,
                    priority: TodoPriority::High,
                },
                TodoItem {
                    content: "Task 2".into(),
                    status: TodoStatus::Completed,
                    priority: TodoPriority::Low,
                },
            ],
        };
        let json = serde_json::to_string(&input).expect("serialize");
        assert!(json.contains("ses_001"));
        assert!(json.contains("Task 1"));
        assert!(json.contains("Task 2"));
    }

    #[test]
    fn test_todo_updated_event() {
        let event = TodoUpdatedEvent {
            session_id: "ses_001".into(),
            todos: vec![TodoItem {
                content: "Test".into(),
                status: TodoStatus::Pending,
                priority: TodoPriority::Medium,
            }],
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("ses_001"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_todo_error_display() {
        let err = TodoError {
            message: "Session not found".into(),
        };
        assert_eq!(err.to_string(), "TodoError: Session not found");
    }
}
