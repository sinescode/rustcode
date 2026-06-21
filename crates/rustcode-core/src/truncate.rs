//! Tool output truncation service — manages truncation of tool output
//! and writes overflow to the truncation directory.
//!
//! Ported from: `packages/opencode/src/tool/truncate.ts`
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum lines retained per tool output.
pub const MAX_LINES: usize = 2000;

/// Maximum characters retained per tool output (50 KB).
pub const MAX_CHARS: usize = 50 * 1024;

/// Directory name for truncation overflow files.
pub const DIR: &str = "tool-output";

/// Glob pattern for truncation files.
pub const GLOB: &str = "tool-output/**/*";

/// Default retention period in hours (7 days).
const DEFAULT_RETENTION_HOURS: u64 = 24 * 7;

/// Configuration for the truncation service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncateOptions {
    /// Maximum lines per tool output (default: 2000)
    pub max_lines: usize,
    /// Maximum characters per tool output (default: 50 KB)
    pub max_chars: usize,
    /// Directory to write overflow files
    pub dir: PathBuf,
    /// Retention period in hours (default: 168 = 7 days)
    pub retention_hours: u64,
}

impl Default for TruncateOptions {
    fn default() -> Self {
        Self {
            max_lines: MAX_LINES,
            max_chars: MAX_CHARS,
            dir: PathBuf::from(DIR),
            retention_hours: DEFAULT_RETENTION_HOURS,
        }
    }
}

/// Result from the truncation service.
#[derive(Debug, Clone)]
pub struct TruncateResult {
    /// The (possibly truncated) content
    pub content: String,
    /// Whether the output was truncated
    pub truncated: bool,
    /// Path to the full output file, if written
    pub output_path: Option<String>,
}

/// The truncation service manages tool output truncation and overflow file writing.
///
/// # Source
/// Ported from `packages/opencode/src/tool/truncate.ts` — Service interface.
pub struct TruncateService {
    options: RwLock<TruncateOptions>,
}

impl TruncateService {
    /// Create a new truncation service with default options.
    pub fn new() -> Self {
        Self {
            options: RwLock::new(TruncateOptions::default()),
        }
    }

    /// Create a new truncation service with custom options.
    pub fn with_options(options: TruncateOptions) -> Self {
        Self {
            options: RwLock::new(options),
        }
    }

    /// Return a reference to the current options (synchronous access).
    pub fn options(&self) -> &RwLock<TruncateOptions> {
        &self.options
    }

    /// Get current limits.
    pub async fn limits(&self) -> (usize, usize) {
        let opts = self.options.read().await;
        (opts.max_lines, opts.max_chars)
    }

    /// Write overflow content to the truncation directory.
    /// Returns the path to the written file.
    pub async fn write(&self, session_id: &str, tool_call_id: &str, content: &str) -> std::io::Result<String> {
        let opts = self.options.read().await;
        let dir = opts.dir.join(session_id);
        std::fs::create_dir_all(&dir)?;
        let file_path = dir.join(format!("{}.txt", tool_call_id));
        std::fs::write(&file_path, content)?;
        Ok(file_path.to_string_lossy().to_string())
    }

    /// Read the full output from a truncation file.
    pub async fn output(&self, session_id: &str, tool_call_id: &str) -> std::io::Result<String> {
        let opts = self.options.read().await;
        let file_path = opts.dir.join(session_id).join(format!("{}.txt", tool_call_id));
        std::fs::read_to_string(&file_path)
    }

    /// Truncate tool output to fit within configured limits.
    /// If truncated, writes the full output to the truncation directory.
    pub async fn truncate(
        &self,
        output: &str,
        session_id: &str,
        tool_call_id: &str,
    ) -> TruncateResult {
        let opts = self.options.read().await;
        let lines: Vec<&str> = output.lines().collect();
        let total_chars: usize = output.chars().count();

        if total_chars <= opts.max_chars && lines.len() <= opts.max_lines {
            return TruncateResult {
                content: output.to_string(),
                truncated: false,
                output_path: None,
            };
        }

        let output_path = match std::fs::create_dir_all(opts.dir.join(session_id)) {
            Ok(()) => {
                let file_path = opts.dir.join(session_id).join(format!("{}.txt", tool_call_id));
                match std::fs::write(&file_path, output) {
                    Ok(()) => Some(file_path.to_string_lossy().to_string()),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        };

        let truncated_lines = std::cmp::min(lines.len(), opts.max_lines);
        let mut result = String::new();
        let mut char_count = 0;

        for (i, line) in lines.iter().enumerate().take(truncated_lines) {
            let line_chars = line.chars().count() + 1;
            if char_count + line_chars > opts.max_chars {
                let remaining = opts.max_chars - char_count;
                if remaining > 20 {
                    result.push_str(&line.chars().take(remaining).collect::<String>());
                    result.push_str("\n... (output truncated)");
                } else {
                    result.push_str("... (output truncated)");
                }
                return TruncateResult {
                    content: result,
                    truncated: true,
                    output_path,
                };
            }
            result.push_str(line);
            if i < truncated_lines - 1 {
                result.push('\n');
            }
            char_count += line_chars;
        }

        if lines.len() > truncated_lines {
            result.push_str(&format!(
                "\n... (output truncated: {} lines > {} limit. Full output written to: {})",
                lines.len(),
                opts.max_lines,
                output_path.as_deref().unwrap_or("(unavailable)")
            ));
        }

        TruncateResult {
            content: result,
            truncated: true,
            output_path,
        }
    }

    /// Get the directory path for a given session.
    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        // Use a cheap default approach
        PathBuf::from(DIR).join(session_id)
    }

    /// List truncation files for a session.
    pub async fn list(&self, session_id: &str) -> std::io::Result<Vec<PathBuf>> {
        let dir = self.session_dir(session_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        let mut read_dir = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            if entry.file_type().await?.is_file() {
                files.push(entry.path());
            }
        }
        files.sort();
        Ok(files)
    }

    /// Delete truncation files for a specific tool call.
    pub async fn delete(&self, session_id: &str, tool_call_id: &str) -> std::io::Result<bool> {
        let opts = self.options.read().await;
        let file_path = opts.dir.join(session_id).join(format!("{}.txt", tool_call_id));
        if file_path.exists() {
            std::fs::remove_file(&file_path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Clean up truncation files older than the configured retention period.
    pub async fn cleanup(&self) -> std::io::Result<usize> {
        let opts = self.options.read().await;
        let retention = std::time::Duration::from_secs(opts.retention_hours * 3600);
        let now = std::time::SystemTime::now();
        let mut removed = 0usize;

        let dir = &opts.dir;
        if !dir.exists() {
            return Ok(0);
        }

        let walker = walkdir::WalkDir::new(dir).into_iter();
        for entry in walker.filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(duration) = now.duration_since(modified) {
                            if duration > retention {
                                let _ = std::fs::remove_file(entry.path());
                                removed += 1;
                            }
                        }
                    }
                }
            }
        }

        Self::remove_empty_dirs(dir);
        Ok(removed)
    }

    /// Schedule periodic cleanup.
    pub fn start_background_cleanup(service: Arc<Self>, interval_hours: u64) {
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(interval_hours * 3600);
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = service.cleanup().await {
                    tracing::warn!(error = %e, "truncation cleanup failed");
                }
            }
        });
    }

    fn remove_empty_dirs(dir: &std::path::Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::remove_empty_dirs(&path);
                }
            }
        }
        let _ = std::fs::remove_dir(dir);
    }
}

impl Default for TruncateService {
    fn default() -> Self {
        Self::new()
    }
}

/// Synchronous truncation of output (for use without the service).
#[must_use]
pub fn truncate_output(output: &str, max_lines: usize, max_chars: usize) -> TruncateResult {
    let lines: Vec<&str> = output.lines().collect();
    let total_chars: usize = output.chars().count();

    if total_chars <= max_chars && lines.len() <= max_lines {
        return TruncateResult {
            content: output.to_string(),
            truncated: false,
            output_path: None,
        };
    }

    let truncated_lines = std::cmp::min(lines.len(), max_lines);
    let mut result = String::new();
    let mut char_count = 0;

    for (i, line) in lines.iter().enumerate().take(truncated_lines) {
        let line_chars = line.chars().count() + 1;
        if char_count + line_chars > max_chars {
            let remaining = max_chars - char_count;
            if remaining > 20 {
                result.push_str(&line.chars().take(remaining).collect::<String>());
                result.push_str("\n... (output truncated)");
            } else {
                result.push_str("... (output truncated)");
            }
            return TruncateResult {
                content: result,
                truncated: true,
                output_path: None,
            };
        }
        result.push_str(line);
        if i < truncated_lines - 1 {
            result.push('\n');
        }
        char_count += line_chars;
    }

    if lines.len() > truncated_lines {
        result.push_str(&format!(
            "\n... (truncated: {} lines > {} limit)",
            lines.len(),
            max_lines
        ));
    }

    TruncateResult {
        content: result,
        truncated: true,
        output_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_truncation_needed() {
        let svc = TruncateService::new();
        let result = svc.truncate("short output", "s1", "t1").await;
        assert!(!result.truncated);
        assert_eq!(result.content, "short output");
        assert!(result.output_path.is_none());
    }

    #[tokio::test]
    async fn test_truncate_by_lines() {
        let svc = TruncateService::with_options(TruncateOptions {
            max_chars: 1_000_000,
            max_lines: 3,
            ..Default::default()
        });
        let input = "line1\nline2\nline3\nline4\nline5";
        let result = svc.truncate(input, "s1", "t1").await;
        assert!(result.truncated);
        assert!(result.content.contains("line1"));
        assert!(result.content.contains("truncated"));
    }

    #[tokio::test]
    async fn test_truncate_by_chars() {
        let svc = TruncateService::with_options(TruncateOptions {
            max_chars: 10,
            max_lines: 1000,
            ..Default::default()
        });
        let input = "this is a long line that should be truncated by char limit";
        let result = svc.truncate(input, "s1", "t1").await;
        assert!(result.truncated);
        assert!(result.content.len() <= 50);
    }

    #[test]
    fn test_truncate_output_fn() {
        let result = truncate_output("short", 10, 100);
        assert!(!result.truncated);
        assert_eq!(result.content, "short");
    }
}
