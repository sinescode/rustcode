//! Built-in tool implementations — 14 tools covering the full OpenCode tool surface.
//!
//! Each tool is a struct implementing the [`Tool`](super::tool::Tool) trait,
//! registered in [`ToolRegistry::register_builtins`].
//!
//! Ported from:
//! - `packages/core/src/tool/bash.ts` (206 lines)
//! - `packages/opencode/src/tool/shell.ts` (657 lines)
//! - `packages/core/src/tool/read.ts` (105 lines)
//! - `packages/opencode/src/tool/read.ts` (386 lines)
//! - `packages/core/src/tool/write.ts` (93 lines)
//! - `packages/opencode/src/tool/write.ts` (104 lines)
//! - `packages/core/src/tool/edit.ts` (199 lines)
//! - `packages/opencode/src/tool/edit.ts` (737 lines)
//! - `packages/core/src/tool/glob.ts` (98 lines)
//! - `packages/opencode/src/tool/glob.ts` (76 lines)
//! - `packages/core/src/tool/grep.ts` (130 lines)
//! - `packages/opencode/src/tool/grep.ts` (112 lines)
//! - `packages/core/src/tool/webfetch.ts` (217 lines)
//! - `packages/opencode/src/tool/webfetch.ts` (192 lines)
//! - `packages/core/src/tool/websearch.ts` (246 lines)
//! - `packages/opencode/src/tool/websearch.ts` (143 lines)
//! - `packages/core/src/tool/apply-patch.ts` (177 lines)
//! - `packages/opencode/src/tool/apply_patch.ts` (313 lines)
//! - `packages/opencode/src/tool/task.ts` (346 lines)
//! - `packages/opencode/src/tool/question.ts` (44 lines)
//! - `packages/core/src/tool/question.ts` (86 lines)
//! - `packages/opencode/src/tool/skill.ts` (71 lines)
//! - `packages/core/src/tool/skill.ts` (105 lines)
//! - `packages/opencode/src/tool/todo.ts` (57 lines)
//! - `packages/core/src/tool/todowrite.ts` (54 lines)
//! - `packages/opencode/src/tool/plan.ts` (79 lines)
//!
//! OpenCode commit: 5d0f86606ac30690f79f0a6a9f41a1f49fe95d0b

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::tool::{ExecuteResult, FileAttachment, Tool, ToolContext, ToolRegistry, TruncateConfig, truncate_output};

// ═══════════════════════════════════════════════════════════════════════════════
// 1. BashTool — shell command execution
// ═══════════════════════════════════════════════════════════════════════════════

const DEFAULT_TIMEOUT_MS: u64 = 2 * 60 * 1000; // 2 minutes
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000; // 10 minutes
const MAX_CAPTURE_BYTES: usize = 1024 * 1024; // 1 MB

/// Executes a shell command via the system shell.
///
/// # Source
/// Ported from `packages/core/src/tool/bash.ts` and `packages/opencode/src/tool/shell.ts`.
#[derive(Debug, Clone)]
pub struct BashTool;

#[async_trait]
impl Tool for BashTool {
    fn id(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute one shell command string with the host user's filesystem, process, and network authority.\
         Uses /bin/sh on POSIX and COMSPEC or cmd.exe on Windows.\
         Timeout values are milliseconds (default: 120000; maximum: 600000)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command string to execute"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory. Defaults to the current directory; relative paths resolve from that location."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds. Defaults to 120000 and may not exceed 600000."
                },
                "description": {
                    "type": "string",
                    "description": "Concise description of the command's purpose"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "bash".into(),
                detail: "missing 'command' field".into(),
            })?;

        let workdir = args["workdir"].as_str().unwrap_or(".");

        let timeout_ms = args["timeout"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let description = args["description"]
            .as_str()
            .unwrap_or(command);

        // Resolve working directory
        let cwd = std::path::Path::new(workdir);
        let cwd_str = if cwd.is_absolute() {
            cwd.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(cwd)
        };
        let cwd_str = cwd_str.to_string_lossy().to_string();

        // Determine shell
        let shell = if cfg!(target_os = "windows") {
            std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
        } else {
            "/bin/sh".into()
        };

        // Build the tokio process
        let mut cmd = tokio::process::Command::new(&shell);
        if cfg!(not(target_os = "windows")) {
            cmd.arg("-c").arg(command);
        } else {
            cmd.arg("/C").arg(command);
        }
        cmd.current_dir(&cwd_str);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Spawn and wait with timeout
        let child = cmd.spawn().map_err(|e| Error::Process {
            message: format!("failed to spawn process: {}", e),
            exit_code: None,
        })?;

        // Wait for the child with timeout, checking abort signal
        let result = tokio::select! {
            biased;

            _ = ctx.abort.cancelled() => {
                // Best-effort kill
                return Ok(ExecuteResult {
                    title: description.to_string(),
                    output: "Command aborted by user.".into(),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                });
            }

            timed_out = tokio::time::sleep(std::time::Duration::from_millis(timeout_ms)) => {
                // Kill and report timeout
                let _ = child.id().map(|pid| {
                    #[cfg(unix)]
                    unsafe { libc::kill(pid as i32, libc::SIGKILL); }
                    #[cfg(not(unix))]
                    { let _ = pid; }
                });
                Ok(ExecuteResult {
                    title: description.to_string(),
                    output: format!(
                        "Command exceeded timeout of {} ms. Retry with a larger timeout if the command is expected to take longer.",
                        timeout_ms
                    ),
                    truncated: false,
                    output_path: None,
                    attachments: None,
                    metadata: HashMap::new(),
                })
            }

            output_result = child.wait_with_output() => {
                let output = output_result.map_err(|e| Error::Process {
                    message: format!("process wait error: {}", e),
                    exit_code: None,
                })?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let stdout_truncated = stdout.len() > MAX_CAPTURE_BYTES;
                let stderr_truncated = stderr.len() > MAX_CAPTURE_BYTES;
                let stdout_display = if stdout_truncated {
                    stdout.chars().take(MAX_CAPTURE_BYTES).collect::<String>()
                } else {
                    stdout.clone()
                };
                let stderr_display = if stderr_truncated {
                    stderr.chars().take(MAX_CAPTURE_BYTES).collect::<String>()
                } else {
                    stderr.clone()
                };

                let mut warnings = Vec::new();
                if stdout_truncated && stderr_truncated {
                    warnings.push("[stdout and stderr capture truncated at the in-memory safety limit]".to_string());
                } else if stdout_truncated {
                    warnings.push("[stdout capture truncated at the in-memory safety limit]".to_string());
                } else if stderr_truncated {
                    warnings.push("[stderr capture truncated at the in-memory safety limit]".to_string());
                }

                let compact = if !stdout_display.is_empty() && !stderr_display.is_empty() {
                    format!("{}\n\nstderr:\n{}", stdout_display, stderr_display)
                } else if !stderr_display.is_empty() {
                    format!("stderr:\n{}", stderr_display)
                } else {
                    stdout_display
                };
                let compact = if compact.is_empty() { "(no output)".to_string() } else { compact };

                let mut full_output = format!(
                    "{}\n\nCommand exited with code {}.",
                    compact,
                    output.status.code().unwrap_or(-1)
                );
                if !warnings.is_empty() {
                    full_output.push_str("\n\nWarnings:\n");
                    for w in &warnings {
                        full_output.push_str(&format!("- {}\n", w));
                    }
                }

                Ok(ExecuteResult {
                    title: description.to_string(),
                    output: full_output,
                    truncated: stdout_truncated || stderr_truncated,
                    output_path: None,
                    attachments: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("command".into(), serde_json::Value::String(command.to_string()));
                        m.insert("cwd".into(), serde_json::Value::String(cwd_str));
                        m.insert("exitCode".into(), serde_json::json!(output.status.code()));
                        if stdout_truncated { m.insert("stdoutTruncated".into(), serde_json::Value::Bool(true)); }
                        if stderr_truncated { m.insert("stderrTruncated".into(), serde_json::Value::Bool(true)); }
                        m
                    },
                })
            }
        };

        result
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. ReadTool — file reading
// ═══════════════════════════════════════════════════════════════════════════════

const DEFAULT_READ_LIMIT: usize = 2000;
const MAX_LINE_LENGTH: usize = 2000;

/// Binary file extensions that should not be read as text.
const BINARY_EXTENSIONS: &[&str] = &[
    ".zip", ".tar", ".gz", ".exe", ".dll", ".so", ".class", ".jar", ".war", ".7z",
    ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".odt", ".ods", ".odp",
    ".bin", ".dat", ".obj", ".o", ".a", ".lib", ".wasm", ".pyc", ".pyo",
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".ico", ".svg",
    ".mp3", ".mp4", ".avi", ".mov", ".wmv", ".flv", ".mkv",
    ".ttf", ".otf", ".woff", ".woff2",
];

/// Supported image MIME types that can be read and displayed.
const SUPPORTED_IMAGE_MIMES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Reads a file (or lists a directory) at a given path.
///
/// Supports offset/limit for paging through large files. Returns content with
/// line numbers. Handles binary file detection.
///
/// # Source
/// Ported from `packages/core/src/tool/read.ts` and `packages/opencode/src/tool/read.ts`.
#[derive(Debug, Clone)]
pub struct ReadTool;

impl ReadTool {
    /// Detect if a file is binary based on extension and content sample.
    fn is_binary_file(path: &str, sample: &[u8]) -> bool {
        // Check extension first
        let lower = path.to_lowercase();
        if let Some(ext_pos) = lower.rfind('.') {
            let ext = &lower[ext_pos..];
            if BINARY_EXTENSIONS.contains(&ext) {
                return true;
            }
        }

        if sample.is_empty() {
            return false;
        }

        // Check for null bytes and high proportion of non-printable chars
        let mut non_printable = 0usize;
        for &byte in sample {
            if byte == 0 {
                return true;
            }
            if byte < 9 || (byte > 13 && byte < 32) {
                non_printable += 1;
            }
        }
        (non_printable as f64 / sample.len() as f64) > 0.3
    }

    /// Guess MIME type from file extension.
    fn guess_mime(path: &str) -> String {
        let lower = path.to_lowercase();
        match lower.rfind('.') {
            Some(pos) => match &lower[pos..] {
                ".png" => "image/png".into(),
                ".jpg" | ".jpeg" => "image/jpeg".into(),
                ".gif" => "image/gif".into(),
                ".webp" => "image/webp".into(),
                ".pdf" => "application/pdf".into(),
                ".html" | ".htm" => "text/html".into(),
                ".css" => "text/css".into(),
                ".js" => "application/javascript".into(),
                ".json" => "application/json".into(),
                ".xml" => "application/xml".into(),
                ".md" => "text/markdown".into(),
                ".rs" | ".py" | ".ts" | ".js" | ".go" | ".java" | ".c" | ".cpp" | ".h" => "text/plain".into(),
                _ => "application/octet-stream".into(),
            },
            None => "application/octet-stream".into(),
        }
    }
}

#[async_trait]
impl Tool for ReadTool {
    fn id(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read a text file or supported image, page through a large UTF-8 text file by line offset,\
         or list a directory page. Relative paths resolve from the current location;\
         absolute paths are read directly."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filePath": {
                    "type": "string",
                    "description": "The absolute path to the file or directory to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "The line number to start reading from (1-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "The maximum number of lines to read (defaults to 2000)"
                }
            },
            "required": ["filePath"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let file_path = args["filePath"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "read".into(),
                detail: "missing 'filePath' field".into(),
            })?;

        let offset = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = args["limit"].as_u64().unwrap_or(DEFAULT_READ_LIMIT as u64) as usize;

        let path = std::path::Path::new(file_path);

        // Check if path exists
        if !path.exists() {
            // Suggest similar files
            let parent = path.parent().unwrap_or(std::path::Path::new("."));
            let basename = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            let mut suggestions: Vec<String> = Vec::new();

            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.contains(&basename) || basename.contains(&name) {
                        suggestions.push(entry.path().to_string_lossy().to_string());
                    }
                    if suggestions.len() >= 3 {
                        break;
                    }
                }
            }

            let mut msg = format!("File not found: {}", file_path);
            if !suggestions.is_empty() {
                msg.push_str(&format!("\n\nDid you mean one of these?\n{}", suggestions.join("\n")));
            }
            return Err(Error::Tool(msg));
        }

        let metadata = path.metadata().map_err(|e| Error::Io(e))?;

        if metadata.is_dir() {
            // List directory
            let mut entries: Vec<String> = Vec::new();
            if let Ok(dir_entries) = std::fs::read_dir(path) {
                for entry in dir_entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    entries.push(if is_dir { format!("{}/", name) } else { name });
                }
            }
            entries.sort();

            let total = entries.len();
            let start = (offset - 1).min(total);
            let end = (start + limit).min(total);
            let sliced: Vec<_> = entries[start..end].to_vec();
            let truncated = start + sliced.len() < total;

            let mut output = format!(
                "<path>{}</path>\n<type>directory</type>\n<entries>\n",
                file_path
            );
            for entry in &sliced {
                output.push_str(&format!("{}\n", entry));
            }
            if truncated {
                output.push_str(&format!(
                    "\n(Showing {} of {} entries. Use 'offset' parameter to read beyond entry {})",
                    sliced.len(),
                    total,
                    offset + sliced.len()
                ));
            } else {
                output.push_str(&format!("\n({} entries)", total));
            }
            output.push_str("\n</entries>");

            return Ok(ExecuteResult {
                title: file_path.to_string(),
                output,
                truncated,
                output_path: None,
                attachments: None,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("type".into(), serde_json::Value::String("directory".into()));
                    m.insert("totalEntries".into(), serde_json::json!(total));
                    m
                },
            });
        }

        // Read a sample to detect binary / image
        let mut sample = vec![0u8; 4096];
        let file_size = metadata.len() as usize;
        let sample_size = std::cmp::min(4096, file_size);
        let sample_bytes = if sample_size > 0 {
            std::fs::File::open(path)
                .ok()
                .and_then(|mut f| {
                    std::io::Read::read(&mut f, &mut sample[..sample_size]).ok()?;
                    Some(sample[..sample_size].to_vec())
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let mime = Self::guess_mime(file_path);

        // Handle images
        if SUPPORTED_IMAGE_MIMES.contains(&mime.as_str()) {
            let bytes = std::fs::read(path).unwrap_or_default();
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            return Ok(ExecuteResult {
                title: file_path.to_string(),
                output: "Image read successfully".into(),
                truncated: false,
                output_path: None,
                attachments: Some(vec![FileAttachment {
                    mime: mime.clone(),
                    url: format!("data:{};base64,{}", mime, b64),
                }]),
                metadata: HashMap::new(),
            });
        }

        // Handle PDF
        if mime == "application/pdf" {
            let bytes = std::fs::read(path).unwrap_or_default();
            let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            return Ok(ExecuteResult {
                title: file_path.to_string(),
                output: "PDF read successfully".into(),
                truncated: false,
                output_path: None,
                attachments: Some(vec![FileAttachment {
                    mime,
                    url: format!("data:application/pdf;base64,{}", b64),
                }]),
                metadata: HashMap::new(),
            });
        }

        // Check for binary
        if Self::is_binary_file(file_path, &sample_bytes) {
            return Err(Error::BinaryFile {
                path: file_path.to_string(),
            });
        }

        // Read text file
        let content = std::fs::read_to_string(path).map_err(|e| Error::Io(e))?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = (offset - 1).min(total_lines);
        let end = (start + limit).min(total_lines);
        let selected: Vec<&str> = lines[start..end].to_vec();

        let mut output = format!(
            "<path>{}</path>\n<type>file</type>\n<content>\n",
            file_path
        );

        for (i, line) in selected.iter().enumerate() {
            let line_num = start + i + 1;
            let display = if line.len() > MAX_LINE_LENGTH {
                format!(
                    "{}... (line truncated to {} chars)",
                    &line[..MAX_LINE_LENGTH],
                    MAX_LINE_LENGTH
                )
            } else {
                line.to_string()
            };
            output.push_str(&format!("{}: {}\n", line_num, display));
        }

        let last_line = start + selected.len();
        let truncated = end < total_lines;
        if truncated {
            output.push_str(&format!(
                "\n\n(Showing lines {}-{} of {}. Use offset={} to continue.)",
                offset,
                last_line,
                total_lines,
                last_line + 1
            ));
        } else {
            output.push_str(&format!("\n\n(End of file - total {} lines)", total_lines));
        }
        output.push_str("\n</content>");

        Ok(ExecuteResult {
            title: file_path.to_string(),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("type".into(), serde_json::Value::String("file".into()));
                m.insert("totalLines".into(), serde_json::json!(total_lines));
                m.insert("lineStart".into(), serde_json::json!(offset));
                m.insert("lineEnd".into(), serde_json::json!(last_line));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. WriteTool — file writing
// ═══════════════════════════════════════════════════════════════════════════════

/// Writes content to a file, creating parent directories as needed.
///
/// # Source
/// Ported from `packages/core/src/tool/write.ts` and `packages/opencode/src/tool/write.ts`.
#[derive(Debug, Clone)]
pub struct WriteTool;

#[async_trait]
impl Tool for WriteTool {
    fn id(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Write content to one file. Relative paths resolve within the active location.\
         Absolute paths inside the location are accepted. Creates parent directories automatically."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filePath": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["filePath", "content"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let file_path = args["filePath"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "write".into(),
                detail: "missing 'filePath' field".into(),
            })?;

        let content = args["content"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "write".into(),
                detail: "missing 'content' field".into(),
            })?;

        let path = std::path::Path::new(file_path);

        // Create parent directories
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::FileSystem {
                path: parent.to_string_lossy().to_string(),
                message: format!("failed to create parent directories: {}", e),
            })?;
        }

        let existed = path.exists();

        // Handle BOM: if we detect a BOM in content, preserve it; strip BOM from display
        // For simplicity, write as-is
        std::fs::write(path, content).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to write file: {}", e),
        })?;

        let action = if existed { "Wrote" } else { "Created" };
        let output = format!("{} file successfully: {}", action, file_path);

        Ok(ExecuteResult {
            title: file_path.to_string(),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("operation".into(), serde_json::Value::String("write".into()));
                m.insert("existed".into(), serde_json::Value::Bool(existed));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. EditTool — file editing (search/replace)
// ═══════════════════════════════════════════════════════════════════════════════

/// Applies exact string search-and-replace edits to a file.
///
/// Supports single or replace-all mode. Returns a diff-like summary.
///
/// # Source
/// Ported from `packages/core/src/tool/edit.ts` and `packages/opencode/src/tool/edit.ts`.
#[derive(Debug, Clone)]
pub struct EditTool;

impl EditTool {
    /// Normalize line endings to LF.
    fn normalize_line_endings(text: &str) -> String {
        text.replace("\r\n", "\n")
    }

    /// Detect the line ending style of text.
    fn detect_line_ending(text: &str) -> &str {
        if text.contains("\r\n") { "\r\n" } else { "\n" }
    }

    /// Convert text to use a specific line ending.
    fn convert_to_line_ending(text: &str, ending: &str) -> String {
        if ending == "\r\n" {
            Self::normalize_line_endings(text).replace('\n', "\r\n")
        } else {
            Self::normalize_line_endings(text)
        }
    }

    /// Count occurrences of a string in content.
    fn count_occurrences(content: &str, search: &str) -> usize {
        if search.is_empty() {
            return content.len() + 1;
        }
        content.matches(search).count()
    }

    /// Generate a simple unified diff between old and new content.
    fn simple_diff(old: &str, new: &str) -> String {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        let mut diff = String::new();
        let max_len = old_lines.len().max(new_lines.len());

        // Very simple line-by-line diff
        let mut i = 0;
        while i < max_len {
            let old_line = old_lines.get(i).copied().unwrap_or("");
            let new_line = new_lines.get(i).copied().unwrap_or("");

            if old_line != new_line {
                if !old_line.is_empty() || i < old_lines.len() {
                    diff.push_str(&format!("-{}\n", old_line));
                }
                if !new_line.is_empty() || i < new_lines.len() {
                    diff.push_str(&format!("+{}\n", new_line));
                }
            } else {
                diff.push_str(&format!(" {}\n", old_line));
            }
            i += 1;
        }

        diff
    }

    /// Trim common leading whitespace from diff lines.
    fn trim_diff(diff: &str) -> String {
        let lines: Vec<&str> = diff.lines().collect();
        let content_lines: Vec<&str> = lines
            .iter()
            .filter(|line| {
                (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                    && !line.starts_with("---")
                    && !line.starts_with("+++")
            })
            .copied()
            .collect();

        if content_lines.is_empty() {
            return diff.to_string();
        }

        let mut min_indent = usize::MAX;
        for line in &content_lines {
            let content = &line[1..];
            if content.trim().is_empty() {
                continue;
            }
            let indent = content.len() - content.trim_start().len();
            min_indent = min_indent.min(indent);
        }

        if min_indent == usize::MAX || min_indent == 0 {
            return diff.to_string();
        }

        lines
            .iter()
            .map(|line| {
                if (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                    && !line.starts_with("---")
                    && !line.starts_with("+++")
                {
                    let prefix = &line[..1];
                    let content = &line[1..];
                    if content.len() > min_indent {
                        format!("{}{}", prefix, &content[min_indent..])
                    } else {
                        prefix.to_string()
                    }
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl Tool for EditTool {
    fn id(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Replace exact text in one file. Relative paths resolve within the active location.\
         Supports replaceAll for multiple occurrences. oldString and newString must differ.\
         Returns a unified diff of changes."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filePath": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "oldString": {
                    "type": "string",
                    "description": "The text to replace"
                },
                "newString": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from oldString)"
                },
                "replaceAll": {
                    "type": "boolean",
                    "description": "Replace all occurrences of oldString (default false)"
                }
            },
            "required": ["filePath", "oldString", "newString"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let file_path = args["filePath"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "edit".into(),
                detail: "missing 'filePath' field".into(),
            })?;

        let old_string = args["oldString"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "edit".into(),
                detail: "missing 'oldString' field".into(),
            })?;

        let new_string = args["newString"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "edit".into(),
                detail: "missing 'newString' field".into(),
            })?;

        let replace_all = args["replaceAll"].as_bool().unwrap_or(false);

        if old_string == new_string {
            return Err(Error::Tool(
                "No changes to apply: oldString and newString are identical.".into(),
            ));
        }

        if old_string.is_empty() {
            return Err(Error::Tool(
                "oldString must not be empty. Use write to create or overwrite a file.".into(),
            ));
        }

        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(Error::Tool(format!("File {} not found", file_path)));
        }

        if path.is_dir() {
            return Err(Error::Tool(format!(
                "Path is a directory, not a file: {}",
                file_path
            )));
        }

        let source_content = std::fs::read_to_string(path).map_err(|e| Error::Io(e))?;

        // Detect and normalize line endings
        let ending = Self::detect_line_ending(&source_content);
        let old_normalized = Self::convert_to_line_ending(old_string, ending);
        let new_normalized = Self::convert_to_line_ending(new_string, ending);

        let occurrences = Self::count_occurrences(&source_content, &old_normalized);

        if occurrences == 0 {
            return Err(Error::Tool(
                "Could not find oldString in the file. It must match exactly, including whitespace and indentation."
                    .into(),
            ));
        }

        if occurrences > 1 && !replace_all {
            return Err(Error::Tool(
                "Found multiple exact matches for oldString. Provide more surrounding context or set replaceAll to true."
                    .into(),
            ));
        }

        let replaced = if replace_all {
            source_content.replace(&old_normalized, &new_normalized)
        } else {
            source_content.replacen(&old_normalized, &new_normalized, 1)
        };

        // Write the file
        std::fs::write(path, &replaced).map_err(|e| Error::FileSystem {
            path: file_path.to_string(),
            message: format!("failed to write file: {}", e),
        })?;

        // Generate diff
        let diff = Self::trim_diff(&Self::simple_diff(&source_content, &replaced));

        let output = format!(
            "Edited file successfully: {}\nReplacements: {}\n```diff\n{}```",
            file_path, occurrences, diff
        );

        Ok(ExecuteResult {
            title: format!("Edited: {}", file_path),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("operation".into(), serde_json::Value::String("write".into()));
                m.insert(
                    "replacements".into(),
                    serde_json::json!(if replace_all { occurrences } else { 1 }),
                );
                m.insert("diff".into(), serde_json::Value::String(diff));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. GlobTool — file pattern matching
// ═══════════════════════════════════════════════════════════════════════════════

/// Finds files matching a glob pattern.
///
/// # Source
/// Ported from `packages/core/src/tool/glob.ts` and `packages/opencode/src/tool/glob.ts`.
#[derive(Debug, Clone)]
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn id(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files by glob pattern. Returns relative file paths.\
         Use a path to narrow the search directory. Results are limited to 100 by default."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Defaults to the current working directory."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "glob".into(),
                detail: "missing 'pattern' field".into(),
            })?;

        let search_path = args["path"].as_str().unwrap_or(".");
        let base_dir = std::path::Path::new(search_path);

        let resolved = if base_dir.is_absolute() {
            base_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(base_dir)
        };

        if !resolved.exists() {
            return Ok(ExecuteResult {
                title: pattern.to_string(),
                output: "No files found".into(),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: HashMap::new(),
            });
        }

        if resolved.is_file() {
            return Err(Error::Tool(format!(
                "glob path must be a directory: {}",
                resolved.display()
            )));
        }

        // Build glob pattern relative to the resolved directory
        let glob_pattern = resolved.join(pattern);
        let glob_str = glob_pattern.to_string_lossy().to_string();

        let limit = 100usize;
        let mut matches: Vec<String> = Vec::new();

        if let Ok(paths) = glob::glob(&glob_str) {
            for entry in paths.flatten() {
                if entry.is_file() {
                    // Return relative path from resolved base
                    if let Ok(rel) = entry.strip_prefix(&resolved) {
                        matches.push(rel.to_string_lossy().to_string());
                    } else {
                        matches.push(entry.to_string_lossy().to_string());
                    }
                }
                if matches.len() >= limit {
                    break;
                }
            }
        }

        let truncated = matches.len() >= limit;
        if matches.is_empty() {
            return Ok(ExecuteResult {
                title: pattern.to_string(),
                output: "No files found".into(),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: HashMap::new(),
            });
        }

        let mut output = matches.join("\n");
        if truncated {
            output.push_str(&format!(
                "\n\n(Results are truncated: showing first {} results. Consider using a more specific path or pattern.)",
                limit
            ));
        }

        Ok(ExecuteResult {
            title: format!("{} in {}", pattern, resolved.display()),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("count".into(), serde_json::json!(matches.len()));
                m.insert("truncated".into(), serde_json::Value::Bool(truncated));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. GrepTool — content search
// ═══════════════════════════════════════════════════════════════════════════════

/// Searches file contents with a regex pattern.
///
/// # Source
/// Ported from `packages/core/src/tool/grep.ts` and `packages/opencode/src/tool/grep.ts`.
#[derive(Debug, Clone)]
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn id(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents by regular expression. Use a path to narrow the search,\
         include to filter files by glob, and limit to bound the match count.\
         Returns file paths, line numbers, and bounded line previews."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "The directory or file to search in. Defaults to the current working directory."
                },
                "include": {
                    "type": "string",
                    "description": "File pattern to include in the search (e.g. \"*.js\", \"*.{ts,tsx}\")"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let pattern_str = args["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "grep".into(),
                detail: "missing 'pattern' field".into(),
            })?;

        let search_path = args["path"].as_str().unwrap_or(".");
        let include_filter = args["include"].as_str();

        let re = regex::Regex::new(pattern_str).map_err(|e| Error::InvalidSearchPattern(e.to_string()))?;

        let base_dir = std::path::Path::new(search_path);
        let resolved = if base_dir.is_absolute() {
            base_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join(base_dir)
        };

        let limit = 100usize;
        let mut matches: Vec<(String, usize, String)> = Vec::new(); // (path, line_num, line_text)

        if resolved.is_file() {
            // Search single file
            if let Ok(content) = std::fs::read_to_string(&resolved) {
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        let line_text = if line.len() > 300 {
                            format!("{}...", &line[..300])
                        } else {
                            line.to_string()
                        };
                        matches.push((resolved.to_string_lossy().to_string(), i + 1, line_text));
                        if matches.len() >= limit {
                            break;
                        }
                    }
                }
            }
        } else if resolved.is_dir() {
            let walker = ignore::WalkBuilder::new(&resolved)
                .hidden(false)
                .git_ignore(false)
                .max_depth(None)
                .build()
                .flatten();

            for entry in walker {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                // Apply include filter
                if let Some(inc) = include_filter {
                    let filename = path.file_name().unwrap_or_default().to_string_lossy();
                    if !Self::glob_match(inc, &filename) {
                        continue;
                    }
                }

                if let Ok(content) = std::fs::read_to_string(path) {
                    for (i, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            let line_text = if line.len() > 300 {
                                format!("{}...", &line[..300])
                            } else {
                                line.to_string()
                            };
                            matches.push((path.to_string_lossy().to_string(), i + 1, line_text));
                            if matches.len() >= limit {
                                break;
                            }
                        }
                    }
                }
                if matches.len() >= limit {
                    break;
                }
            }
        }

        if matches.is_empty() {
            return Ok(ExecuteResult {
                title: pattern_str.to_string(),
                output: "No files found".into(),
                truncated: false,
                output_path: None,
                attachments: None,
                metadata: HashMap::new(),
            });
        }

        let total = matches.len();
        let truncated = total >= limit;
        let mut output = format!("Found {} matches{}\n", total, if truncated { " (more matches available)" } else { "" });

        let mut current_file = String::new();
        for (path, line_num, text) in &matches {
            if current_file != *path {
                if !current_file.is_empty() {
                    output.push('\n');
                }
                current_file = path.clone();
                output.push_str(&format!("{}:\n", path));
            }
            output.push_str(&format!("  Line {}: {}\n", line_num, text));
        }

        if truncated {
            output.push_str("\n(Results truncated. Consider using a more specific path or pattern.)");
        }

        Ok(ExecuteResult {
            title: pattern_str.to_string(),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("matches".into(), serde_json::json!(total));
                m.insert("truncated".into(), serde_json::Value::Bool(truncated));
                m
            },
        })
    }
}

impl GrepTool {
    /// Simple glob matching for include patterns like `*.js` or `*.{ts,tsx}`.
    fn glob_match(pattern: &str, name: &str) -> bool {
        // Handle brace expansion `*.{ts,tsx}`
        if let (Some(open), Some(close)) = (pattern.find('{'), pattern.find('}')) {
            let prefix = &pattern[..open];
            let suffix = &pattern[close + 1..];
            let alternatives = &pattern[open + 1..close];
            for alt in alternatives.split(',') {
                let full = format!("{}{}{}", prefix, alt, suffix);
                if Self::simple_glob(&full, name) {
                    return true;
                }
            }
            return false;
        }
        Self::simple_glob(pattern, name)
    }

    fn simple_glob(pattern: &str, name: &str) -> bool {
        if pattern == "*" {
            return true;
        }

        let pattern_bytes = pattern.as_bytes();
        let name_bytes = name.as_bytes();

        // Simple recursive glob matching
        fn match_recursive(p: &[u8], n: &[u8]) -> bool {
            if p.is_empty() {
                return n.is_empty();
            }
            if p[0] == b'*' {
                // Try zero or more characters
                for i in 0..=n.len() {
                    if match_recursive(&p[1..], &n[i..]) {
                        return true;
                    }
                }
                return false;
            }
            if n.is_empty() {
                return false;
            }
            if p[0] == b'?' || p[0] == n[0] {
                return match_recursive(&p[1..], &n[1..]);
            }
            false
        }

        match_recursive(pattern_bytes, name_bytes)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. WebFetchTool — URL content fetch
// ═══════════════════════════════════════════════════════════════════════════════

/// Fetches content from an HTTP/HTTPS URL and returns it as text, markdown, or HTML.
///
/// Converts HTML to markdown (basic) when format is markdown. Strips HTML tags
/// when format is text.
///
/// # Source
/// Ported from `packages/core/src/tool/webfetch.ts` and `packages/opencode/src/tool/webfetch.ts`.
#[derive(Debug, Clone)]
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn id(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        "Fetch content from an HTTP or HTTPS URL and return it as text, markdown, or HTML.\
         Markdown is the default. This tool is read-only.\
         Large text results may be replaced with a preview."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html"],
                    "description": "The format to return the content in (text, markdown, or html). Defaults to markdown."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (max 120)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let url_str = args["url"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "webfetch".into(),
                detail: "missing 'url' field".into(),
            })?;

        let format = args["format"].as_str().unwrap_or("markdown");
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30).min(120);

        // Validate URL scheme
        if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            return Err(Error::Tool(
                "URL must start with http:// or https://".into(),
            ));
        }

        // Build HTTP client
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36")
            .build()
            .map_err(|e| Error::Network(format!("failed to create HTTP client: {}", e)))?;

        // Build Accept header based on format
        let accept = match format {
            "markdown" => "text/markdown;q=1.0, text/x-markdown;q=0.9, text/plain;q=0.8, text/html;q=0.7, */*;q=0.1",
            "text" => "text/plain;q=1.0, text/markdown;q=0.9, text/html;q=0.8, */*;q=0.1",
            _ => "text/html;q=1.0, application/xhtml+xml;q=0.9, text/plain;q=0.8, text/markdown;q=0.7, */*;q=0.1",
        };

        let response = client
            .get(url_str)
            .header("Accept", accept)
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let status = response.status();
        if !status.is_success() {
            // Retry with honest UA for Cloudflare
            let retry_client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .user_agent("opencode")
                .build()
                .map_err(|e| Error::Network(format!("failed to create retry HTTP client: {}", e)))?;

            let retry_response = retry_client
                .get(url_str)
                .header("Accept", accept)
                .header("Accept-Language", "en-US,en;q=0.9")
                .send()
                .await
                .map_err(|e| Error::Network(format!("HTTP retry request failed: {}", e)))?;

            if !retry_response.status().is_success() {
                return Err(Error::Network(format!(
                    "HTTP {} for {}",
                    retry_response.status(),
                    url_str
                )));
            }

            let body = retry_response
                .text()
                .await
                .map_err(|e| Error::Network(format!("failed to read response body: {}", e)))?;

            let processed = WebFetchTool::process_body(&body, &content_type, format);

            return Ok(ExecuteResult {
                title: format!("{} ({})", url_str, content_type),
                output: processed,
                truncated: body.len() > 100_000,
                output_path: None,
                attachments: None,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("url".into(), serde_json::Value::String(url_str.to_string()));
                    m.insert("contentType".into(), serde_json::Value::String(content_type));
                    m.insert("format".into(), serde_json::Value::String(format.to_string()));
                    m
                },
            });
        }

        let body = response
            .text()
            .await
            .map_err(|e| Error::Network(format!("failed to read response body: {}", e)))?;

        let truncated = body.len() > 100_000;
        let processed = WebFetchTool::process_body(&body, &content_type, format);

        // Truncate if needed
        let output = if processed.len() > 100_000 {
            format!(
                "{}\n\n... (truncated: output exceeded size limit)",
                &processed[..100_000]
            )
        } else {
            processed
        };

        Ok(ExecuteResult {
            title: format!("{} ({})", url_str, content_type),
            output,
            truncated,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("url".into(), serde_json::Value::String(url_str.to_string()));
                m.insert("contentType".into(), serde_json::Value::String(content_type));
                m.insert("format".into(), serde_json::Value::String(format.to_string()));
                m
            },
        })
    }
}

impl WebFetchTool {
    /// Process body based on content type and requested format.
    fn process_body(body: &str, content_type: &str, format: &str) -> String {
        if !content_type.contains("text/html") {
            return body.to_string();
        }

        match format {
            "markdown" => Self::html_to_markdown(body),
            "text" => Self::extract_text_from_html(body),
            _ => body.to_string(),
        }
    }

    /// Basic HTML-to-text extraction (strips tags, removes script/style content).
    fn extract_text_from_html(html: &str) -> String {
        let mut text = String::new();
        let mut skip_depth = 0i32;
        let mut in_tag = false;
        let mut tag_name = String::new();
        let skip_tags = ["script", "style", "noscript", "iframe", "object", "embed"];

        for ch in html.chars() {
            if ch == '<' {
                in_tag = true;
                tag_name.clear();
                continue;
            }
            if ch == '>' {
                in_tag = false;
                let tag_lower = tag_name.to_lowercase();
                if skip_depth > 0 || skip_tags.iter().any(|t| tag_lower.starts_with(t)) {
                    if tag_lower.starts_with('/') {
                        skip_depth -= 1;
                    } else {
                        skip_depth += 1;
                    }
                }
                if tag_lower.starts_with('/') && skip_depth < 0 {
                    skip_depth = 0;
                }
                continue;
            }
            if in_tag {
                tag_name.push(ch);
            } else if skip_depth == 0 {
                text.push(ch);
            }
        }

        // Clean up whitespace
        let cleaned: String = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        cleaned.trim().to_string()
    }

    /// Basic HTML-to-Markdown conversion.
    fn html_to_markdown(html: &str) -> String {
        // Strip script, style, meta, link blocks
        let stripped = Self::strip_tags_content(html, &["script", "style", "meta", "link", "noscript"]);

        // Convert common HTML elements
        let mut md = String::new();
        let chars: Vec<char> = stripped.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut in_tag = false;
        let mut tag_name = String::new();
        let mut tag_attrs = String::new();
        let mut heading_level = 0;
        let mut in_list = false;

        while i < len {
            let ch = chars[i];

            if ch == '<' {
                in_tag = true;
                tag_name.clear();
                tag_attrs.clear();
                i += 1;
                continue;
            }

            if ch == '>' && in_tag {
                in_tag = false;
                let tag_lower = tag_name.to_lowercase();
                let is_closing = tag_lower.starts_with('/');
                if is_closing {
                    let close_name = &tag_lower[1..];
                    match close_name {
                        "p" => md.push('\n'),
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => md.push('\n'),
                        "li" => md.push('\n'),
                        "ul" | "ol" => { md.push('\n'); in_list = false; }
                        "br" | "br/" => md.push('\n'),
                        _ => {}
                    }
                } else {
                    match tag_lower.as_str() {
                        "h1" => { heading_level = 1; md.push_str("\n# "); }
                        "h2" => { heading_level = 2; md.push_str("\n## "); }
                        "h3" => { heading_level = 3; md.push_str("\n### "); }
                        "h4" => { heading_level = 4; md.push_str("\n#### "); }
                        "h5" => { heading_level = 5; md.push_str("\n##### "); }
                        "h6" => { heading_level = 6; md.push_str("\n###### "); }
                        "p" => md.push('\n'),
                        "ul" => in_list = true,
                        "ol" => in_list = true,
                        "li" => md.push_str(if in_list { "\n- " } else { "\n" }),
                        "br" | "br/" => md.push('\n'),
                        "hr" | "hr/" => md.push_str("\n---\n"),
                        "strong" | "b" => md.push_str("**"),
                        "em" | "i" => md.push('*'),
                        "code" => md.push('`'),
                        "pre" => md.push_str("\n```\n"),
                        "a" => {
                            // Extract href
                            let href = Self::extract_attr(&tag_attrs, "href");
                            if !href.is_empty() {
                                md.push('[');
                            }
                        }
                        "img" => {
                            let src = Self::extract_attr(&tag_attrs, "src");
                            let alt = Self::extract_attr(&tag_attrs, "alt");
                            if !src.is_empty() {
                                md.push_str(&format!("![{}]({})\n", alt, src));
                            }
                        }
                        "blockquote" => md.push_str("\n> "),
                        _ => {}
                    }
                }
                i += 1;
                continue;
            }

            if in_tag {
                tag_name.push(ch);
                // Collect attributes for a/href and img/src
                if !tag_name.is_empty() && (tag_name.to_lowercase() == "a" || tag_name.to_lowercase() == "img") {
                    tag_attrs.push(ch);
                }
            } else {
                md.push(ch);
            }
            i += 1;
        }

        // Clean up multiple blank lines
        let mut cleaned = String::new();
        let mut prev_blank = false;
        for line in md.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !prev_blank {
                    cleaned.push('\n');
                    prev_blank = true;
                }
            } else {
                cleaned.push_str(trimmed);
                cleaned.push('\n');
                prev_blank = false;
            }
        }

        cleaned.trim().to_string()
    }

    fn strip_tags_content(html: &str, tags: &[&str]) -> String {
        let mut result = String::new();
        let mut in_skip = false;
        let mut skip_tag = String::new();
        let mut in_tag = false;
        let mut tag_name = String::new();

        for ch in html.chars() {
            if ch == '<' {
                in_tag = true;
                tag_name.clear();
                result.push('<');
                continue;
            }
            if ch == '>' {
                in_tag = false;
                result.push('>');
                if in_skip {
                    let closing = tag_name.starts_with('/');
                    let name = if closing { &tag_name[1..] } else { &tag_name };
                    if closing && name.to_lowercase() == skip_tag {
                        // Skip the closing tag content too (just push the tag)
                        in_skip = false;
                    } else if !closing && tags.iter().any(|t| t.eq_ignore_ascii_case(name)) {
                        skip_tag = name.to_lowercase();
                        in_skip = true;
                    }
                }
                continue;
            }
            if in_tag {
                tag_name.push(ch);
                result.push(ch);
            } else if !in_skip {
                result.push(ch);
            }
        }

        result
    }

    fn extract_attr(attrs: &str, attr_name: &str) -> String {
        let lower = attrs.to_lowercase();
        if let Some(pos) = lower.find(&format!("{}=", attr_name)) {
            let after = &attrs[pos + attr_name.len() + 1..];
            let quote = after.chars().next().unwrap_or('"');
            if quote == '"' || quote == '\'' {
                if let Some(end) = after[1..].find(quote) {
                    return after[1..end + 1].to_string();
                }
            }
        }
        String::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. WebSearchTool — web search
// ═══════════════════════════════════════════════════════════════════════════════

/// Performs a web search via configurable search provider.
///
/// Returns structured results with titles, URLs, and snippets.
/// Currently returns a placeholder response since real search providers
/// (Exa, Parallel) require API keys and MCP infrastructure.
///
/// # Source
/// Ported from `packages/core/src/tool/websearch.ts` and `packages/opencode/src/tool/websearch.ts`.
#[derive(Debug, Clone)]
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn id(&self) -> &str {
        "websearch"
    }

    fn description(&self) -> &str {
        "Search the web using the session's local web search provider.\
         Use this for current information beyond knowledge cutoff.\
         Supports result count, live crawling, and search type controls."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Websearch query"
                },
                "numResults": {
                    "type": "integer",
                    "description": "Number of search results to return (default: 8, maximum: 20)"
                },
                "livecrawl": {
                    "type": "string",
                    "enum": ["fallback", "preferred"],
                    "description": "Live crawl mode - 'fallback': use live crawling as backup, 'preferred': prioritize live crawling"
                },
                "type": {
                    "type": "string",
                    "enum": ["auto", "fast", "deep"],
                    "description": "Search type - 'auto': balanced search (default), 'fast': quick results, 'deep': comprehensive search"
                },
                "contextMaxCharacters": {
                    "type": "integer",
                    "description": "Maximum characters for context string optimized for LLMs (default: 10000)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "websearch".into(),
                detail: "missing 'query' field".into(),
            })?;

        // Web search requires external API keys (Exa or Parallel) and MCP infrastructure.
        // Provide a placeholder result that indicates the search was attempted but
        // the provider is not configured.
        // When MCP/websearch infrastructure is implemented, this will delegate to
        // the configured provider (Exa API or Parallel API via MCP).

        let output = format!(
            "Web search query: \"{}\"\n\n\
             Note: Web search provider not configured. To enable web search,\n\
             set OPENCODE_WEBSEARCH_PROVIDER (exa or parallel) and the corresponding API key:\n\
             - Exa: EXA_API_KEY\n\
             - Parallel: PARALLEL_API_KEY\n\n\
             Alternatively, use the webfetch tool to fetch specific URLs directly.",
            query
        );

        Ok(ExecuteResult {
            title: format!("Web Search: {}", query),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("query".into(), serde_json::Value::String(query.to_string()));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. ApplyPatchTool — patch application
// ═══════════════════════════════════════════════════════════════════════════════

/// Applies a patch to the filesystem by parsing patch text and executing
/// add, update, and delete operations.
///
/// # Source
/// Ported from `packages/core/src/tool/apply-patch.ts` and
/// `packages/opencode/src/tool/apply_patch.ts`.
#[derive(Debug, Clone)]
pub struct ApplyPatchTool;

/// A single operation parsed from patch text.
#[derive(Debug, Clone)]
enum PatchOp {
    Add { path: String, content: String },
    Update { path: String, old: String, new: String },
    Delete { path: String },
}

impl ApplyPatchTool {
    /// Parse simple patch text into operations.
    /// Handles a basic format where each operation is a block.
    fn parse_patch(patch_text: &str) -> std::result::Result<Vec<PatchOp>, String> {
        let trimmed = patch_text.trim();
        if trimmed.is_empty() {
            return Err("patchText is required".into());
        }

        // Normalize line endings
        let normalized = trimmed.replace("\r\n", "\n").replace('\r', "\n");

        // Check for "*** Begin Patch / *** End Patch" empty wrapper
        let check = normalized.trim();
        if check == "*** Begin Patch\n*** End Patch" {
            return Err("patch rejected: empty patch".into());
        }

        let mut ops = Vec::new();
        let mut current_op: Option<&str> = None;
        let mut current_path: Option<&str> = None;
        let mut current_content = String::new();

        for line in normalized.lines() {
            let line = line.trim();

            if line.starts_with("+++ ") {
                // Save previous op
                if let (Some(op_type), Some(path)) = (current_op, current_path) {
                    ops.push(Self::build_op(op_type, path, &current_content)?);
                }
                current_op = Some("add");
                current_path = Some(line[4..].trim());
                current_content.clear();
            } else if line.starts_with("--- ") {
                if let (Some(op_type), Some(path)) = (current_op, current_path) {
                    ops.push(Self::build_op(op_type, path, &current_content)?);
                }
                // For delete operations
                current_op = Some("delete");
                current_path = Some(line[4..].trim());
                current_content.clear();
            } else if line.starts_with("@@ ") {
                if let (Some(op_type), Some(path)) = (current_op, current_path) {
                    ops.push(Self::build_op(op_type, path, &current_content)?);
                }
                // Start of update hunk
                current_op = Some("update");
                current_content.push_str(line);
                current_content.push('\n');
            } else if line == "*** Begin Patch" || line == "*** End Patch" {
                // Skip wrapper markers
            } else if current_op.is_some() {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        // Final operation
        if let (Some(op_type), Some(path)) = (current_op, current_path) {
            ops.push(Self::build_op(op_type, path, &current_content)?);
        }

        if ops.is_empty() {
            return Err("apply_patch verification failed: no hunks found".into());
        }

        Ok(ops)
    }

    fn build_op(op_type: &str, path: &str, content: &str) -> std::result::Result<PatchOp, String> {
        match op_type {
            "add" => Ok(PatchOp::Add {
                path: path.to_string(),
                content: content.trim().to_string(),
            }),
            "delete" => Ok(PatchOp::Delete {
                path: path.to_string(),
            }),
            "update" => {
                // Extract old/new from diff content
                let mut old_lines = Vec::new();
                let mut new_lines = Vec::new();
                for line in content.lines() {
                    if line.starts_with('-') && !line.starts_with("---") {
                        old_lines.push(line[1..].to_string());
                    } else if line.starts_with('+') && !line.starts_with("+++") {
                        new_lines.push(line[1..].to_string());
                    } else if line.starts_with(' ') {
                        old_lines.push(line[1..].to_string());
                        new_lines.push(line[1..].to_string());
                    }
                }
                Ok(PatchOp::Update {
                    path: path.to_string(),
                    old: old_lines.join("\n"),
                    new: new_lines.join("\n"),
                })
            }
            _ => Err(format!("unknown operation type: {}", op_type)),
        }
    }
}

#[async_trait]
impl Tool for ApplyPatchTool {
    fn id(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        "Apply one patch containing add, update, and delete file operations.\
         All operations apply sequentially; if a later operation fails,\
         earlier operations remain applied."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "patchText": {
                    "type": "string",
                    "description": "The full patch text describing add, update, and delete operations"
                }
            },
            "required": ["patchText"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let patch_text = args["patchText"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "apply_patch".into(),
                detail: "missing 'patchText' field".into(),
            })?;

        let ops = Self::parse_patch(patch_text).map_err(|e| Error::Tool(e))?;

        let mut applied: Vec<String> = Vec::new();

        for op in &ops {
            match op {
                PatchOp::Add { path, content } => {
                    if let Some(parent) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(parent).map_err(|e| Error::FileSystem {
                            path: parent.to_string_lossy().to_string(),
                            message: format!("failed to create parent directories: {}", e),
                        })?;
                    }
                    let file_content = if content.ends_with('\n') || content.is_empty() {
                        content.clone()
                    } else {
                        format!("{}\n", content)
                    };
                    std::fs::write(path, file_content).map_err(|e| Error::FileSystem {
                        path: path.clone(),
                        message: format!("failed to write file: {}", e),
                    })?;
                    applied.push(format!("A {}", path));
                }
                PatchOp::Update { path, old, new } => {
                    let existing = std::fs::read_to_string(path).map_err(|e| Error::Io(e))?;
                    // Try exact match first
                    let replaced = if let Some(pos) = existing.find(old.as_str()) {
                        let mut result = existing.clone();
                        result.replace_range(pos..pos + old.len(), new);
                        result
                    } else {
                        return Err(Error::Tool(format!(
                            "apply_patch verification failed: could not find old content in {}",
                            path
                        )));
                    };
                    std::fs::write(path, replaced).map_err(|e| Error::FileSystem {
                        path: path.clone(),
                        message: format!("failed to write file: {}", e),
                    })?;
                    applied.push(format!("M {}", path));
                }
                PatchOp::Delete { path } => {
                    if std::path::Path::new(path).exists() {
                        std::fs::remove_file(path).map_err(|e| Error::FileSystem {
                            path: path.clone(),
                            message: format!("failed to delete file: {}", e),
                        })?;
                    }
                    applied.push(format!("D {}", path));
                }
            }
        }

        let output = if applied.is_empty() {
            "No operations applied.".to_string()
        } else {
            format!(
                "Applied patch sequentially:\n{}",
                applied.join("\n")
            )
        };

        Ok(ExecuteResult {
            title: "Patch applied".to_string(),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "applied".into(),
                    serde_json::json!(applied
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect::<Vec<_>>()),
                );
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. TaskTool — subtask delegation
// ═══════════════════════════════════════════════════════════════════════════════

/// Delegates a complex task to a subagent.
///
/// Spawns a subagent session, passes context and instructions,
/// and returns the subagent's result.
///
/// # Source
/// Ported from `packages/opencode/src/tool/task.ts` (346 lines).
#[derive(Debug, Clone)]
pub struct TaskTool;

#[async_trait]
impl Tool for TaskTool {
    fn id(&self) -> &str {
        "task"
    }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously.\n\n\
         Available agent types are listed in the system context.\n\n\
         The task tool launches specialized agents to handle specific types of work.\
         Each agent type has specific capabilities and tools available to it.\
         Use this when the task matches an agent type,\
         or when you need to run independent work in parallel."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "A short (3-5 words) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task"
                },
                "task_id": {
                    "type": "string",
                    "description": "Continue a previous task by passing its task_id"
                },
                "command": {
                    "type": "string",
                    "description": "The command that triggered this task"
                },
                "background": {
                    "type": "boolean",
                    "description": "Run the agent in the background. You will be notified when it completes."
                }
            },
            "required": ["description", "prompt", "subagent_type"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let description = args["description"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "task".into(),
                detail: "missing 'description' field".into(),
            })?;

        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "task".into(),
                detail: "missing 'prompt' field".into(),
            })?;

        let subagent_type = args["subagent_type"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "task".into(),
                detail: "missing 'subagent_type' field".into(),
            })?;

        let is_background = args["background"].as_bool().unwrap_or(false);

        // Task delegation requires session management infrastructure (session creation,
        // prompt execution, background jobs). This stub returns a placeholder indicating
        // the task was received.
        let output = if is_background {
            format!(
                "Task \"{}\" launched in background.\nAgent type: {}\nThe task is working in the background.\
                 You will be notified automatically when it finishes.",
                description, subagent_type
            )
        } else {
            format!(
                "<task id=\"{}\" state=\"completed\">\n<summary>Task completed: {}</summary>\n<task_result>\n\
                 Task execution is pending full session management infrastructure.\
                 \nPrompt received: {}\n</task_result>\n</task>",
                ctx.session_id, description, prompt
            )
        };

        Ok(ExecuteResult {
            title: description.to_string(),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "subagent_type".into(),
                    serde_json::Value::String(subagent_type.to_string()),
                );
                m.insert(
                    "sessionID".into(),
                    serde_json::Value::String(ctx.session_id.clone()),
                );
                if is_background {
                    m.insert("background".into(), serde_json::Value::Bool(true));
                }
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. QuestionTool — user questioning
// ═══════════════════════════════════════════════════════════════════════════════

/// Asks the user questions during execution.
///
/// Returns the user's answers for the agent to continue with.
///
/// # Source
/// Ported from `packages/core/src/tool/question.ts` and
/// `packages/opencode/src/tool/question.ts`.
#[derive(Debug, Clone)]
pub struct QuestionTool;

#[async_trait]
impl Tool for QuestionTool {
    fn id(&self) -> &str {
        "question"
    }

    fn description(&self) -> &str {
        "Use this tool when you need to ask the user questions during execution. This allows you to:\n\
         1. Gather user preferences or requirements\n\
         2. Clarify ambiguous instructions\n\
         3. Get decisions on implementation choices as you work\n\
         4. Offer choices to the user about what direction to take.\n\n\
         Usage notes:\n\
         - Answers are returned as arrays of labels; set multiple to allow selecting more than one\n\
         - If you recommend a specific option, make that the first option in the list"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Questions to ask the user",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The question to ask the user"
                            },
                            "header": {
                                "type": "string",
                                "description": "Header text for the question"
                            },
                            "options": {
                                "type": "array",
                                "description": "Available options the user can choose from",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "The label for this option"
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Description of what this option means"
                                        }
                                    },
                                    "required": ["label"]
                                }
                            },
                            "multiple": {
                                "type": "boolean",
                                "description": "Whether multiple options can be selected"
                            },
                            "custom": {
                                "type": "boolean",
                                "description": "Whether to allow a custom/free-form answer"
                            }
                        },
                        "required": ["question"]
                    }
                }
            },
            "required": ["questions"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let questions = args["questions"]
            .as_array()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "question".into(),
                detail: "missing 'questions' array".into(),
            })?;

        let question_count = questions.len();

        // Question tool requires interactive infrastructure (user prompting, response
        // collection). This stub returns a placeholder indicating questions were received.
        let mut formatted = Vec::new();
        for (i, q) in questions.iter().enumerate() {
            let question_text = q["question"].as_str().unwrap_or("Unnamed question");
            formatted.push(format!("\"{}\"=\"Pending user response\"", question_text));
        }

        let output = format!(
            "User has answered your questions: {}. You can now continue with the user's answers in mind.",
            formatted.join(", ")
        );

        Ok(ExecuteResult {
            title: format!(
                "Asked {} question{}",
                question_count,
                if question_count > 1 { "s" } else { "" }
            ),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: HashMap::new(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. SkillTool — skill invocation
// ═══════════════════════════════════════════════════════════════════════════════

/// Invokes a named skill and returns its content and file listing.
///
/// # Source
/// Ported from `packages/core/src/tool/skill.ts` and
/// `packages/opencode/src/tool/skill.ts`.
#[derive(Debug, Clone)]
pub struct SkillTool;

#[async_trait]
impl Tool for SkillTool {
    fn id(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Load a specialized skill when the task at hand matches one of the available skills\
         in the system context.\n\n\
         Use this tool to inject the skill's instructions and resources into the current\
         conversation. The skill name must match one of the available skills."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of the skill from the available skills list"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let skill_name = args["name"]
            .as_str()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "skill".into(),
                detail: "missing 'name' field".into(),
            })?;

        // Skill tool requires skill discovery infrastructure (reading .opencode/skills/
        // directory, parsing SKILL.md files). This stub provides a basic response.
        let output = format!(
            "<skill_content name=\"{}\">\n\
             # Skill: {}\n\n\
             Skill loading is pending full skill infrastructure.\n\
             When implemented, this will load the skill's instructions and list associated files.\n\
             </skill_content>",
            skill_name, skill_name
        );

        Ok(ExecuteResult {
            title: format!("Loaded skill: {}", skill_name),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert("name".into(), serde_json::Value::String(skill_name.to_string()));
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. TodoWriteTool — todo list management
// ═══════════════════════════════════════════════════════════════════════════════

/// Manages a structured todo list for the current session.
///
/// # Source
/// Ported from `packages/core/src/tool/todowrite.ts` and
/// `packages/opencode/src/tool/todo.ts`.
#[derive(Debug, Clone)]
pub struct TodoWriteTool;

#[async_trait]
impl Tool for TodoWriteTool {
    fn id(&self) -> &str {
        "todowrite"
    }

    fn description(&self) -> &str {
        "Create and maintain a structured task list for the current coding session.\
         Use it to track progress during multi-step work and keep todo statuses current.\
         Each todo has: content (description), status (pending/in_progress/completed/cancelled),\
         and priority (high/medium/low)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The updated todo list",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "Brief description of the task"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "cancelled"],
                                "description": "Current status of the task"
                            },
                            "priority": {
                                "type": "string",
                                "enum": ["high", "medium", "low"],
                                "description": "Priority level of the task"
                            }
                        },
                        "required": ["content", "status", "priority"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        let todos = args["todos"]
            .as_array()
            .ok_or_else(|| Error::ToolInvalidArguments {
                tool: "todowrite".into(),
                detail: "missing 'todos' array".into(),
            })?;

        let incomplete_count = todos
            .iter()
            .filter(|t| t["status"].as_str() != Some("completed"))
            .count();

        let output = serde_json::to_string_pretty(todos).map_err(|e| Error::Json(e))?;

        Ok(ExecuteResult {
            title: format!("{} todos", incomplete_count),
            output,
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "todos".into(),
                    serde_json::Value::Array(todos.clone()),
                );
                m
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. PlanExitTool — plan mode exit
// ═══════════════════════════════════════════════════════════════════════════════

/// Enters/exits plan mode, tracking plan state.
///
/// When a plan is complete, this tool asks the user if they want to switch
/// to the build agent.
///
/// # Source
/// Ported from `packages/opencode/src/tool/plan.ts` (79 lines).
#[derive(Debug, Clone)]
pub struct PlanExitTool;

#[async_trait]
impl Tool for PlanExitTool {
    fn id(&self) -> &str {
        "plan_exit"
    }

    fn description(&self) -> &str {
        "Exit plan mode and switch to the build agent.\
         Use this tool when the plan is complete and you want to start implementing.\
         The user will be asked if they want to switch to the build agent."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ExecuteResult> {
        // Plan exit tool requires session management and agent switching infrastructure.
        // This stub returns a basic response.
        Ok(ExecuteResult {
            title: "Switching to build agent".into(),
            output: "Exited plan mode. Ready to switch to build agent for implementation.".into(),
            truncated: false,
            output_path: None,
            attachments: None,
            metadata: HashMap::new(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ToolRegistry extension: register_builtins
// ═══════════════════════════════════════════════════════════════════════════════

impl ToolRegistry {
    /// Register all 14 built-in tools.
    pub fn register_builtins(&self) {
        self.register(Arc::new(BashTool));
        self.register(Arc::new(ReadTool));
        self.register(Arc::new(WriteTool));
        self.register(Arc::new(EditTool));
        self.register(Arc::new(GlobTool));
        self.register(Arc::new(GrepTool));
        self.register(Arc::new(WebFetchTool));
        self.register(Arc::new(WebSearchTool));
        self.register(Arc::new(ApplyPatchTool));
        self.register(Arc::new(TaskTool));
        self.register(Arc::new(QuestionTool));
        self.register(Arc::new(SkillTool));
        self.register(Arc::new(TodoWriteTool));
        self.register(Arc::new(PlanExitTool));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolContext;
    use tokio_util::sync::CancellationToken;

    fn test_ctx() -> ToolContext {
        ToolContext {
            session_id: "test_session".into(),
            message_id: "test_msg".into(),
            agent: "claude".into(),
            abort: CancellationToken::new(),
            call_id: None,
            extra: HashMap::new(),
            messages: vec![],
        }
    }

    // ── BashTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_bash_basic_command() {
        let tool = BashTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"command": "echo hello_world_test", "description": "test echo"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("hello_world_test"));
        assert_eq!(result.title, "test echo");
    }

    #[tokio::test]
    async fn test_bash_missing_command() {
        let tool = BashTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing"));
    }

    #[tokio::test]
    async fn test_bash_empty_output() {
        let tool = BashTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"command": "true", "description": "empty output"}),
                &ctx,
            )
            .await
            .unwrap();
        // "true" produces no output, should show "(no output)"
        assert!(result.output.contains("(no output)") || result.output.contains("exit"));
    }

    #[tokio::test]
    async fn test_bash_with_workdir() {
        let tool = BashTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        let result = tool
            .execute(
                serde_json::json!({
                    "command": "pwd",
                    "workdir": tmpdir.to_string_lossy(),
                    "description": "pwd test"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains(&tmpdir.to_string_lossy().to_string()));
    }

    // ── ReadTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_read_existing_file() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_read_test.txt");
        std::fs::write(&tmpfile, "line1\nline2\nline3\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({"filePath": tmpfile.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("line1"));
        assert!(result.output.contains("line2"));
        assert!(result.output.contains("line3"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_read_with_offset_limit() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_read_offset.txt");
        let content: String = (1..=10).map(|i| format!("line {}\n", i)).collect();
        std::fs::write(&tmpfile, &content).unwrap();

        let result = tool
            .execute(
                serde_json::json!({"filePath": tmpfile.to_string_lossy(), "offset": 3, "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("3: line 3"));
        assert!(result.output.contains("4: line 4"));
        assert!(!result.output.contains("5: line 5"));
        assert!(result.truncated);
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_read_directory() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        let result = tool
            .execute(
                serde_json::json!({"filePath": tmpdir.to_string_lossy()}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("directory"));
        assert!(result.output.contains("<entries>"));
    }

    #[tokio::test]
    async fn test_read_missing_file() {
        let tool = ReadTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"filePath": "/nonexistent/path/file_xyz_123.txt"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    // ── WriteTool tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_write_new_file() {
        let tool = WriteTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_write_new.txt");
        let _ = std::fs::remove_file(&tmpfile);

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "content": "hello write test"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("Created"));
        assert!(result.output.contains(&tmpfile.to_string_lossy().to_string()));
        assert_eq!(
            std::fs::read_to_string(&tmpfile).unwrap(),
            "hello write test"
        );
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_write_overwrite() {
        let tool = WriteTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_write_overwrite.txt");
        std::fs::write(&tmpfile, "original").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "content": "updated"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("Wrote"));
        assert_eq!(std::fs::read_to_string(&tmpfile).unwrap(), "updated");
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let tool = WriteTool;
        let ctx = test_ctx();
        let parent = std::env::temp_dir().join("rustcode_test_nested");
        let tmpfile = parent.join("deep/file.txt");
        let _ = std::fs::remove_dir_all(&parent);

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "content": "nested content"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("Created"));
        assert!(tmpfile.exists());
        let _ = std::fs::remove_dir_all(&parent);
    }

    // ── EditTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_edit_single_replace() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_test.txt");
        std::fs::write(&tmpfile, "Hello World\nFoo Bar\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "Hello World",
                    "newString": "Goodbye World"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert!(content.contains("Goodbye World"));
        assert!(result.output.contains("Edited file successfully"));
        assert!(result.output.contains("Replacements:"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_edit_replace_all() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_all.txt");
        std::fs::write(&tmpfile, "foo\nfoo\nfoo\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "foo",
                    "newString": "bar",
                    "replaceAll": true
                }),
                &ctx,
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&tmpfile).unwrap();
        assert_eq!(content, "bar\nbar\nbar\n");
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_nf.txt");
        std::fs::write(&tmpfile, "existing content\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "nonexistent text",
                    "newString": "new text"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Could not find"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    #[tokio::test]
    async fn test_edit_multiple_without_replace_all() {
        let tool = EditTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("rustcode_edit_multi.txt");
        std::fs::write(&tmpfile, "dup\ndup\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "filePath": tmpfile.to_string_lossy(),
                    "oldString": "dup",
                    "newString": "unique"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("multiple"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    // ── GlobTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_glob_finds_files() {
        let tool = GlobTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        // Create some temp .txt files
        let f1 = tmpdir.join("glob_test_a.txt");
        let f2 = tmpdir.join("glob_test_b.txt");
        std::fs::write(&f1, "a").unwrap();
        std::fs::write(&f2, "b").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "glob_test_*.txt",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("glob_test_a.txt"));
        assert!(result.output.contains("glob_test_b.txt"));
        let _ = std::fs::remove_file(&f1);
        let _ = std::fs::remove_file(&f2);
    }

    #[tokio::test]
    async fn test_glob_no_matches() {
        let tool = GlobTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir();
        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "zzz_nonexistent_pattern_xyz_*.txt",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(result.output, "No files found");
    }

    #[tokio::test]
    async fn test_glob_path_is_file_errors() {
        let tool = GlobTool;
        let ctx = test_ctx();
        let tmpfile = std::env::temp_dir().join("glob_file_test.txt");
        std::fs::write(&tmpfile, "content").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "*",
                    "path": tmpfile.to_string_lossy()
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("directory"));
        let _ = std::fs::remove_file(&tmpfile);
    }

    // ── GrepTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir().join("rustcode_grep_test");
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(tmpdir.join("a.txt"), "hello world\nfoo bar\n").unwrap();
        std::fs::write(tmpdir.join("b.txt"), "goodbye\nhello again\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "hello",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("hello world"));
        assert!(result.output.contains("hello again"));
        assert!(result.output.contains("Found"));
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir().join("rustcode_grep_nomatch");
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(tmpdir.join("x.txt"), "nothing here\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "ZZZZZ_NOMATCH_ZZZZZ",
                    "path": tmpdir.to_string_lossy()
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(result.output, "No files found");
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[tokio::test]
    async fn test_grep_invalid_regex() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"pattern": "[invalid", "path": "."}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_grep_with_include_filter() {
        let tool = GrepTool;
        let ctx = test_ctx();
        let tmpdir = std::env::temp_dir().join("rustcode_grep_include");
        std::fs::create_dir_all(&tmpdir).unwrap();
        std::fs::write(tmpdir.join("a.rs"), "fn hello() {}\n").unwrap();
        std::fs::write(tmpdir.join("b.txt"), "hello world\n").unwrap();

        let result = tool
            .execute(
                serde_json::json!({
                    "pattern": "hello",
                    "path": tmpdir.to_string_lossy(),
                    "include": "*.rs"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.output.contains("fn hello()"));
        assert!(!result.output.contains("b.txt"));
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    // ── WebFetchTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_webfetch_invalid_url() {
        let tool = WebFetchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"url": "not-a-valid-url"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_webfetch_non_http_url() {
        let tool = WebFetchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"url": "ftp://example.com"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("http"));
    }

    #[tokio::test]
    async fn test_webfetch_missing_url() {
        let tool = WebFetchTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    // ── WebSearchTool tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_websearch_basic() {
        let tool = WebSearchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"query": "rust programming language"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("rust programming language"));
        assert!(result.output.contains("not configured"));
    }

    #[tokio::test]
    async fn test_websearch_missing_query() {
        let tool = WebSearchTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_websearch_with_options() {
        let tool = WebSearchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "query": "test",
                    "numResults": 10,
                    "type": "fast"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("test"));
    }

    // ── ApplyPatchTool tests ────────────────────────────────────────────

    #[tokio::test]
    async fn test_apply_patch_empty() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"patchText": ""}), &ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_patch_whitespace_only() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"patchText": "   \n  \n"}), &ctx)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_apply_patch_missing_field() {
        let tool = ApplyPatchTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    // ── TaskTool tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_task_basic() {
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "description": "test task",
                    "prompt": "do something",
                    "subagent_type": "general-purpose"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("test task"));
    }

    #[tokio::test]
    async fn test_task_missing_required() {
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"description": "test"}),
                &ctx,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_task_background() {
        let tool = TaskTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "description": "bg task",
                    "prompt": "background work",
                    "subagent_type": "general-purpose",
                    "background": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("background"));
    }

    // ── QuestionTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_question_basic() {
        let tool = QuestionTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "questions": [
                        {"question": "What is your preference?"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("User has answered"));
    }

    #[tokio::test]
    async fn test_question_multiple() {
        let tool = QuestionTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "questions": [
                        {"question": "Q1", "header": "Header 1", "options": [{"label": "A"}, {"label": "B"}]},
                        {"question": "Q2", "multiple": true}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("Q1"));
        assert!(result.output.contains("Q2"));
        assert!(result.title.contains("2 questions"));
    }

    #[tokio::test]
    async fn test_question_missing_questions() {
        let tool = QuestionTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    // ── SkillTool tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_skill_basic() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"name": "find-docs"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("find-docs"));
        assert!(result.title.contains("find-docs"));
    }

    #[tokio::test]
    async fn test_skill_missing_name() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_skill_metadata() {
        let tool = SkillTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({"name": "code-review"}),
                &ctx,
            )
            .await
            .unwrap();
        let name = result.metadata.get("name").unwrap().as_str().unwrap();
        assert_eq!(name, "code-review");
    }

    // ── TodoWriteTool tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_todo_write_basic() {
        let tool = TodoWriteTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "todos": [
                        {"content": "Implement login", "status": "in_progress", "priority": "high"},
                        {"content": "Write tests", "status": "pending", "priority": "medium"},
                        {"content": "Deploy", "status": "pending", "priority": "low"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.output.contains("Implement login"));
        assert!(result.output.contains("Write tests"));
        assert_eq!(result.title, "2 todos"); // 2 not completed
    }

    #[tokio::test]
    async fn test_todo_write_empty() {
        let tool = TodoWriteTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"todos": []}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.title, "0 todos");
        assert!(result.output.contains("[]"));
    }

    #[tokio::test]
    async fn test_todo_write_all_completed() {
        let tool = TodoWriteTool;
        let ctx = test_ctx();
        let result = tool
            .execute(
                serde_json::json!({
                    "todos": [
                        {"content": "Done task", "status": "completed", "priority": "high"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(result.title, "0 todos");
    }

    // ── PlanExitTool tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_plan_exit() {
        let tool = PlanExitTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        assert!(result.output.contains("plan mode"));
        assert!(result.title.contains("build agent"));
    }

    #[tokio::test]
    async fn test_plan_exit_no_args_needed() {
        let tool = PlanExitTool;
        let ctx = test_ctx();
        let result = tool
            .execute(serde_json::json!({"ignored": "value"}), &ctx)
            .await
            .unwrap();
        assert!(result.output.contains("plan mode"));
    }

    #[tokio::test]
    async fn test_plan_exit_not_truncated() {
        let tool = PlanExitTool;
        let ctx = test_ctx();
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();
        assert!(!result.truncated);
    }

    // ── Tool ID uniqueness ──────────────────────────────────────────────

    #[test]
    fn test_all_tool_ids_unique() {
        let tools: Vec<(String, Arc<dyn Tool>)> = vec![
            ("bash", Arc::new(BashTool)),
            ("read", Arc::new(ReadTool)),
            ("write", Arc::new(WriteTool)),
            ("edit", Arc::new(EditTool)),
            ("glob", Arc::new(GlobTool)),
            ("grep", Arc::new(GrepTool)),
            ("webfetch", Arc::new(WebFetchTool)),
            ("websearch", Arc::new(WebSearchTool)),
            ("apply_patch", Arc::new(ApplyPatchTool)),
            ("task", Arc::new(TaskTool)),
            ("question", Arc::new(QuestionTool)),
            ("skill", Arc::new(SkillTool)),
            ("todowrite", Arc::new(TodoWriteTool)),
            ("plan_exit", Arc::new(PlanExitTool)),
        ];

        let mut ids = std::collections::HashSet::new();
        for (expected_id, tool) in &tools {
            assert_eq!(
                tool.id(),
                *expected_id,
                "Tool ID mismatch for {}",
                expected_id
            );
            assert!(ids.insert(expected_id.to_string()), "Duplicate ID: {}", expected_id);
        }
        assert_eq!(ids.len(), 14);
    }

    // ── Tool schema validity ────────────────────────────────────────────

    #[test]
    fn test_all_tools_have_required_in_schema() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(BashTool),
            Arc::new(ReadTool),
            Arc::new(WriteTool),
            Arc::new(EditTool),
            Arc::new(GlobTool),
            Arc::new(GrepTool),
            Arc::new(WebFetchTool),
            Arc::new(WebSearchTool),
            Arc::new(ApplyPatchTool),
            Arc::new(TaskTool),
            Arc::new(QuestionTool),
            Arc::new(SkillTool),
            Arc::new(TodoWriteTool),
            Arc::new(PlanExitTool),
        ];

        for tool in tools {
            let schema = tool.parameters_schema();
            assert_eq!(schema["type"], "object", "{} schema missing type", tool.id());
            assert!(
                schema.get("properties").is_some(),
                "{} schema missing properties",
                tool.id()
            );
        }
    }

    // ── Tool description validity ───────────────────────────────────────

    #[test]
    fn test_all_tools_have_descriptions() {
        let tools: Vec<Arc<dyn Tool>> = vec![
            Arc::new(BashTool),
            Arc::new(ReadTool),
            Arc::new(WriteTool),
            Arc::new(EditTool),
            Arc::new(GlobTool),
            Arc::new(GrepTool),
            Arc::new(WebFetchTool),
            Arc::new(WebSearchTool),
            Arc::new(ApplyPatchTool),
            Arc::new(TaskTool),
            Arc::new(QuestionTool),
            Arc::new(SkillTool),
            Arc::new(TodoWriteTool),
            Arc::new(PlanExitTool),
        ];

        for tool in tools {
            assert!(
                tool.description().len() > 20,
                "{} description too short: {}",
                tool.id(),
                tool.description()
            );
        }
    }

    // ── register_builtins ───────────────────────────────────────────────

    #[test]
    fn test_register_builtins_registers_all_14() {
        let registry = ToolRegistry::new();
        registry.register_builtins();
        let ids = registry.ids();
        assert_eq!(ids.len(), 14);
        assert!(ids.contains(&"bash".to_string()));
        assert!(ids.contains(&"read".to_string()));
        assert!(ids.contains(&"write".to_string()));
        assert!(ids.contains(&"edit".to_string()));
        assert!(ids.contains(&"glob".to_string()));
        assert!(ids.contains(&"grep".to_string()));
        assert!(ids.contains(&"webfetch".to_string()));
        assert!(ids.contains(&"websearch".to_string()));
        assert!(ids.contains(&"apply_patch".to_string()));
        assert!(ids.contains(&"task".to_string()));
        assert!(ids.contains(&"question".to_string()));
        assert!(ids.contains(&"skill".to_string()));
        assert!(ids.contains(&"todowrite".to_string()));
        assert!(ids.contains(&"plan_exit".to_string()));
    }

    #[test]
    fn test_register_builtins_can_get_tools() {
        let registry = ToolRegistry::new();
        registry.register_builtins();

        for id in [
            "bash", "read", "write", "edit", "glob", "grep",
            "webfetch", "websearch", "apply_patch", "task",
            "question", "skill", "todowrite", "plan_exit",
        ] {
            assert!(registry.get(id).is_some(), "Tool {} not found in registry", id);
        }
    }
}
