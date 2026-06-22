//! LSP integration — core types and data structures.
//!
//! Ported from:
//! - `packages/blazecode/src/lsp/lsp.ts`
//! - `packages/blazecode/src/lsp/client.ts`
//! - `packages/blazecode/src/lsp/diagnostic.ts`
//! - `packages/blazecode/src/lsp/server.ts`
//! - `packages/blazecode/src/lsp/language.ts`
//!
//! This module defines the core LSP types used across the system. The actual
//! LSP server integration (spawning language servers, JSON-RPC communication)
//! lives in the `blazecode-lsp` crate.
//!
//! ## Architecture
//!
//! The TS source uses `vscode-languageserver-types` for diagnostic types and
//! `vscode-jsonrpc` for the LSP protocol. This module provides Rust equivalents
//! of those types, plus the [`LspServerInfo`] and [`LspClientInfo`] structures
//! that describe how to connect to and manage language servers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Positioning and range types
// ---------------------------------------------------------------------------

/// A zero-based position in a text document.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Position` type (lines 22–25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspPosition {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based character offset within the line.
    pub character: u32,
}

impl LspPosition {
    /// Create a new position.
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range in a text document, from `start` to `end`.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Range` type (lines 27–31).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspRange {
    /// Start position (inclusive).
    pub start: LspPosition,
    /// End position (exclusive).
    pub end: LspPosition,
}

impl LspRange {
    /// Create a new range.
    pub fn new(start: LspPosition, end: LspPosition) -> Self {
        Self { start, end }
    }
}

/// A location in a document identified by URI and range.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Symbol.location` (lines 36–39).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocation {
    /// Document URI (file:// scheme).
    pub uri: String,
    /// Range within the document.
    pub range: LspRange,
}

// ---------------------------------------------------------------------------
// Diagnostic types
// ---------------------------------------------------------------------------

/// Severity of a diagnostic message, matching LSP protocol values.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/diagnostic.ts` severity map (lines 6–10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    /// Error — severity 1.
    Error = 1,
    /// Warning — severity 2.
    Warning = 2,
    /// Information — severity 3.
    Information = 3,
    /// Hint — severity 4.
    Hint = 4,
}

impl DiagnosticSeverity {
    /// Returns the display label used in diagnostic reports.
    ///
    /// Mirrors the TS `pretty()` function label map.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warning => "WARN",
            Self::Information => "INFO",
            Self::Hint => "HINT",
        }
    }

    /// Convert from the LSP severity integer.
    pub fn from_i32(n: i32) -> Option<Self> {
        match n {
            1 => Some(Self::Error),
            2 => Some(Self::Warning),
            3 => Some(Self::Information),
            4 => Some(Self::Hint),
            _ => None,
        }
    }
}

/// A diagnostic message from a language server.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/client.ts` — re-exports
/// `vscode-languageserver-types` `Diagnostic`. Mirrors the VSCode
/// diagnostic shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
    /// Document URI.
    pub uri: String,
    /// Range in the document.
    pub range: LspRange,
    /// Diagnostic message text.
    pub message: String,
    /// Severity level (defaults to Error if absent).
    #[serde(default = "default_severity")]
    pub severity: DiagnosticSeverity,
    /// Optional diagnostic code (string or number).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<serde_json::Value>,
    /// Optional source of the diagnostic (e.g., "eslint").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn default_severity() -> DiagnosticSeverity {
    DiagnosticSeverity::Error
}

impl LspDiagnostic {
    /// Format the diagnostic as a human-readable string.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/lsp/diagnostic.ts` `pretty()` (lines 5–18).
    pub fn pretty(&self) -> String {
        let severity = self.severity.label();
        let line = self.range.start.line + 1; // 1-based for display
        let col = self.range.start.character + 1;
        format!("{severity} [{line}:{col}] {}", self.message)
    }

    /// Report diagnostics for a file, limited to errors.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/lsp/diagnostic.ts` `report()` (lines 20–27).
    pub fn report(file: &str, issues: &[LspDiagnostic]) -> String {
        let errors: Vec<&LspDiagnostic> = issues
            .iter()
            .filter(|item| item.severity == DiagnosticSeverity::Error)
            .collect();
        if errors.is_empty() {
            return String::new();
        }
        const MAX_PER_FILE: usize = 20;
        let limited = &errors[..usize::min(MAX_PER_FILE, errors.len())];
        let more = errors.len().saturating_sub(MAX_PER_FILE);
        let mut output = format!("<diagnostics file=\"{file}\">\n");
        for diag in limited {
            output.push_str(&diag.pretty());
            output.push('\n');
        }
        if more > 0 {
            output.push_str(&format!("... and {more} more\n"));
        }
        output.push_str("</diagnostics>");
        output
    }
}

// ---------------------------------------------------------------------------
// Symbol types
// ---------------------------------------------------------------------------

/// LSP symbol kind, matching the LSP protocol's `SymbolKind` enum.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `SymbolKind` enum (lines 60–87).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

impl SymbolKind {
    /// Returns the set of "interesting" symbol kinds used for workspace
    /// symbol filtering.
    ///
    /// # Source
    /// Ported from `packages/blazecode/src/lsp/lsp.ts` `kinds` array (lines 89–98).
    pub const INTERESTING_KINDS: &[SymbolKind] = &[
        SymbolKind::Class,
        SymbolKind::Function,
        SymbolKind::Method,
        SymbolKind::Interface,
        SymbolKind::Variable,
        SymbolKind::Constant,
        SymbolKind::Struct,
        SymbolKind::Enum,
    ];

    /// Whether this symbol kind is in the "interesting" set.
    pub fn is_interesting(&self) -> bool {
        Self::INTERESTING_KINDS.contains(self)
    }
}

/// A symbol from the LSP workspace/document symbol response.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Symbol` type (lines 33–41).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
    /// Symbol name.
    pub name: String,
    /// Kind of symbol.
    pub kind: u32,
    /// Location of the symbol.
    pub location: LspLocation,
}

/// A hierarchical document symbol from the LSP response.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `DocumentSymbol` type (lines 43–50).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDocumentSymbol {
    /// Symbol name.
    pub name: String,
    /// Optional detail string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Kind of symbol.
    pub kind: u32,
    /// Full range of the symbol in the document.
    pub range: LspRange,
    /// Selection range (what to highlight).
    #[serde(rename = "selectionRange")]
    pub selection_range: LspRange,
}


// ---------------------------------------------------------------------------
// Hover, Completion, and other LSP result types
// ---------------------------------------------------------------------------

/// A hover result from a language server.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `hover()` return type (line 127).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspHover {
    /// The hover contents (markdown string or markup content).
    #[serde(default)]
    pub contents: serde_json::Value,
    /// Optional range for the hover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<LspRange>,
}

/// A completion item from a language server.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/client.ts` LSP completion protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCompletionItem {
    /// The label of the completion item.
    pub label: String,
    /// The kind of this completion item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<u32>,
    /// The detail description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The documentation string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// The text to be inserted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
}

/// A location link returned from go-to-definition requests.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `definition()` return type (line 128).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocationLink {
    /// The target URI of the definition.
    pub target_uri: String,
    /// The target range of the definition.
    pub target_range: LspRange,
    /// The target selection range of the definition.
    pub target_selection_range: LspRange,
    /// The origin selection range (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_selection_range: Option<LspRange>,
}

/// A call hierarchy item.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `prepareCallHierarchy()` return type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCallHierarchyItem {
    /// The name of the item.
    pub name: String,
    /// The kind of the item.
    pub kind: u32,
    /// The URI of the item.
    pub uri: String,
    /// The range of the item.
    pub range: LspRange,
    /// The selection range of the item.
    pub selection_range: LspRange,
    /// Optional detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A call hierarchy incoming or outgoing call.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `incomingCalls()`/`outgoingCalls()` return type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCallHierarchyCall {
    /// The from item of the call.
    pub from: LspCallHierarchyItem,
    /// The from ranges.
    pub from_ranges: Vec<LspRange>,
}

// ---------------------------------------------------------------------------
// Server info
// ---------------------------------------------------------------------------

/// Describes an LSP server that can be spawned.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/server.ts` `Info` interface (lines 80–86).
/// Also see the concrete server definitions (Typescript, RustAnalyzer, etc.)
/// in the same file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspServerInfo {
    /// Unique server ID (e.g., "typescript", "rust", "eslint").
    pub id: String,
    /// File extensions this server handles (e.g., [".rs"], [".ts", ".tsx"]).
    pub extensions: Vec<String>,
    /// Optional command to launch the server. When present, this server
    /// can be started from user config even without an auto-detected tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    /// Optional environment variables for the server process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Optional initialization options to send with the `initialize` request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization: Option<serde_json::Value>,
    /// Optional project root directory hint. When set, this path (relative or
    /// absolute) is used as the server root. If absent, the workspace root is used.
    /// Ported from: `packages/blazecode/src/lsp/server.ts` `Info.root` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

// ---------------------------------------------------------------------------
// Client info (connection state)
// ---------------------------------------------------------------------------

/// Status of an LSP client connection.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Status` type (lines 52–57).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspStatus {
    /// Server ID.
    pub id: String,
    /// Human-readable server name.
    pub name: String,
    /// Project root path, relative to the workspace directory.
    pub root: String,
    /// Connection status.
    pub status: LspConnectionStatus,
}

/// Connection status for a language server.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Status.status` literals (line 56).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LspConnectionStatus {
    /// Server is connected and operational.
    Connected,
    /// Server encountered an error.
    Error,
}

/// Information about a running LSP client connection.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/client.ts` `Info` type (line 25)
/// and the returned object shape from `create()` (lines 545–648).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspClientInfo {
    /// The server ID this client belongs to.
    pub server_id: String,
    /// Project root directory for this client.
    pub root: String,
    /// Working directory.
    pub directory: String,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// LSP-related event types published on the event bus.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `Event.Updated` (lines 18–20).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LspEvent {
    /// Published when LSP clients are updated (connected/disconnected).
    #[serde(rename = "lsp.updated")]
    Updated {},
}

impl LspEvent {
    /// Event type string for the bus.
    pub const UPDATED: &str = "lsp.updated";
}

// ---------------------------------------------------------------------------
// Language extensions map
// ---------------------------------------------------------------------------

/// Map from file extension to LSP language ID.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/language.ts` `LANGUAGE_EXTENSIONS`
/// const (lines 1–121).
///
/// Lazily initialized on first access using `OnceLock`.
pub fn language_extensions() -> &'static HashMap<&'static str, &'static str> {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static EXTENSIONS: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    EXTENSIONS.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(".abap", "abap");
        m.insert(".bat", "bat");
        m.insert(".bib", "bibtex");
        m.insert(".bibtex", "bibtex");
        m.insert(".clj", "clojure");
        m.insert(".cljs", "clojure");
        m.insert(".cljc", "clojure");
        m.insert(".edn", "clojure");
        m.insert(".coffee", "coffeescript");
        m.insert(".c", "c");
        m.insert(".cpp", "cpp");
        m.insert(".cxx", "cpp");
        m.insert(".cc", "cpp");
        m.insert(".c++", "cpp");
        m.insert(".cs", "csharp");
        m.insert(".csx", "csharp");
        m.insert(".css", "css");
        m.insert(".d", "d");
        m.insert(".pas", "pascal");
        m.insert(".pascal", "pascal");
        m.insert(".diff", "diff");
        m.insert(".patch", "diff");
        m.insert(".dart", "dart");
        m.insert(".dockerfile", "dockerfile");
        m.insert(".ex", "elixir");
        m.insert(".exs", "elixir");
        m.insert(".erl", "erlang");
        m.insert(".hrl", "erlang");
        m.insert(".fs", "fsharp");
        m.insert(".fsi", "fsharp");
        m.insert(".fsx", "fsharp");
        m.insert(".fsscript", "fsharp");
        m.insert(".go", "go");
        m.insert(".gitcommit", "git-commit");
        m.insert(".gitrebase", "git-rebase");
        m.insert(".groovy", "groovy");
        m.insert(".gleam", "gleam");
        m.insert(".hbs", "handlebars");
        m.insert(".handlebars", "handlebars");
        m.insert(".hs", "haskell");
        m.insert(".lhs", "haskell");
        m.insert(".html", "html");
        m.insert(".htm", "html");
        m.insert(".ini", "ini");
        m.insert(".java", "java");
        m.insert(".jl", "julia");
        m.insert(".js", "javascript");
        m.insert(".jsx", "javascriptreact");
        m.insert(".json", "json");
        m.insert(".kt", "kotlin");
        m.insert(".kts", "kotlin");
        m.insert(".tex", "latex");
        m.insert(".latex", "latex");
        m.insert(".less", "less");
        m.insert(".lua", "lua");
        m.insert(".makefile", "makefile");
        m.insert("makefile", "makefile");
        m.insert(".md", "markdown");
        m.insert(".markdown", "markdown");
        m.insert(".m", "objective-c");
        m.insert(".mm", "objective-cpp");
        m.insert(".pl", "perl");
        m.insert(".pm", "perl");
        m.insert(".pm6", "perl6");
        m.insert(".php", "php");
        m.insert(".ps1", "powershell");
        m.insert(".psm1", "powershell");
        m.insert(".pug", "jade");
        m.insert(".jade", "jade");
        m.insert(".py", "python");
        m.insert(".r", "r");
        m.insert(".cshtml", "razor");
        m.insert(".razor", "razor");
        m.insert(".rb", "ruby");
        m.insert(".rake", "ruby");
        m.insert(".gemspec", "ruby");
        m.insert(".ru", "ruby");
        m.insert(".erb", "erb");
        m.insert(".html.erb", "erb");
        m.insert(".js.erb", "erb");
        m.insert(".css.erb", "erb");
        m.insert(".json.erb", "erb");
        m.insert(".rs", "rust");
        m.insert(".scss", "scss");
        m.insert(".sass", "sass");
        m.insert(".scala", "scala");
        m.insert(".shader", "shaderlab");
        m.insert(".sh", "shellscript");
        m.insert(".bash", "shellscript");
        m.insert(".zsh", "shellscript");
        m.insert(".ksh", "shellscript");
        m.insert(".sql", "sql");
        m.insert(".svelte", "svelte");
        m.insert(".swift", "swift");
        m.insert(".ts", "typescript");
        m.insert(".ets", "typescript");
        m.insert(".mts", "typescript");
        m.insert(".cts", "typescript");
        m.insert(".tsx", "typescriptreact");
        m.insert(".mtsx", "typescriptreact");
        m.insert(".ctsx", "typescriptreact");
        m.insert(".xml", "xml");
        m.insert(".xsl", "xsl");
        m.insert(".yaml", "yaml");
        m.insert(".yml", "yaml");
        m.insert(".mjs", "javascript");
        m.insert(".cjs", "javascript");
        m.insert(".vue", "vue");
        m.insert(".zig", "zig");
        m.insert(".zon", "zig");
        m.insert(".astro", "astro");
        m.insert(".ml", "ocaml");
        m.insert(".mli", "ocaml");
        m.insert(".tf", "terraform");
        m.insert(".tfvars", "terraform-vars");
        m.insert(".hcl", "hcl");
        m.insert(".nix", "nix");
        m.insert(".typ", "typst");
        m.insert(".typc", "typst");
        m
    })
}

// ---------------------------------------------------------------------------
// LspBridge — abstract LSP operations for use from tools
// ---------------------------------------------------------------------------

use std::sync::OnceLock;

/// A global bridge for LSP operations, allowing the tool system to invoke
/// LSP features without depending directly on the `blazecode-lsp` crate.
///
/// The `blazecode-lsp` crate registers its implementation at startup via
/// [`set_global_lsp_bridge`].
pub trait LspBridge: Send + Sync {
    /// Perform a workspace symbol search.
    fn workspace_symbols(&self, query: &str) -> Vec<LspSymbol>;
}

static GLOBAL_LSP_BRIDGE: OnceLock<Box<dyn LspBridge>> = OnceLock::new();

/// Register the global LSP bridge implementation.
///
/// Called by the `blazecode-lsp` crate at initialization time.
/// Returns `Ok(())` on success, or `Err` if already registered.
pub fn set_global_lsp_bridge(bridge: Box<dyn LspBridge>) -> std::result::Result<(), &'static str> {
    GLOBAL_LSP_BRIDGE
        .set(bridge)
        .map_err(|_| "LSP bridge already initialized")
}

/// Check whether a global LSP bridge has been registered.
pub fn has_lsp_bridge() -> bool {
    GLOBAL_LSP_BRIDGE.get().is_some()
}

/// Execute a workspace symbol search via the global bridge.
///
/// Returns an empty vec if no bridge is registered.
pub fn global_workspace_symbols(query: &str) -> Vec<LspSymbol> {
    GLOBAL_LSP_BRIDGE
        .get()
        .map(|bridge| bridge.workspace_symbols(query))
        .unwrap_or_default()
}

/// Get the language ID for a file extension.
///
/// Returns `"plaintext"` if the extension is not recognized.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/client.ts` line 560:
/// `const languageId = LANGUAGE_EXTENSIONS[extension] ?? "plaintext"`
pub fn language_id_for_extension(ext: &str) -> &'static str {
    language_extensions()
        .get(ext)
        .copied()
        .unwrap_or("plaintext")
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// LSP initialization error.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/client.ts` `InitializeError` (lines 29–32).
#[derive(Debug, Error)]
#[error("LSP initialization error for server `{server_id}`")]
pub struct InitializeError {
    /// The server that failed to initialize.
    pub server_id: String,
}

// ---------------------------------------------------------------------------
// Input types for LSP operations
// ---------------------------------------------------------------------------

/// Input for a location-based LSP request.
///
/// # Source
/// Ported from `packages/blazecode/src/lsp/lsp.ts` `LocInput` type (line 112).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocInput {
    /// File path.
    pub file: String,
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based character offset.
    pub character: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Position / Range ----------------------------------------------------

    #[test]
    fn test_position_new() {
        let pos = LspPosition::new(10, 5);
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_range_new() {
        let start = LspPosition::new(0, 0);
        let end = LspPosition::new(5, 10);
        let range = LspRange::new(start, end);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 5);
    }

    #[test]
    fn test_range_serialization() {
        let range = LspRange::new(LspPosition::new(1, 2), LspPosition::new(3, 4));
        let json = serde_json::to_string(&range).expect("serialize");
        let deserialized: LspRange = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized, range);
    }

    // -- Diagnostic severity ------------------------------------------------

    #[test]
    fn test_diagnostic_severity_labels() {
        assert_eq!(DiagnosticSeverity::Error.label(), "ERROR");
        assert_eq!(DiagnosticSeverity::Warning.label(), "WARN");
        assert_eq!(DiagnosticSeverity::Information.label(), "INFO");
        assert_eq!(DiagnosticSeverity::Hint.label(), "HINT");
    }

    #[test]
    fn test_diagnostic_severity_from_i32() {
        assert_eq!(
            DiagnosticSeverity::from_i32(1),
            Some(DiagnosticSeverity::Error)
        );
        assert_eq!(
            DiagnosticSeverity::from_i32(2),
            Some(DiagnosticSeverity::Warning)
        );
        assert_eq!(
            DiagnosticSeverity::from_i32(3),
            Some(DiagnosticSeverity::Information)
        );
        assert_eq!(
            DiagnosticSeverity::from_i32(4),
            Some(DiagnosticSeverity::Hint)
        );
        assert_eq!(DiagnosticSeverity::from_i32(0), None);
        assert_eq!(DiagnosticSeverity::from_i32(5), None);
    }

    // -- Diagnostic ---------------------------------------------------------

    #[test]
    fn test_diagnostic_pretty() {
        let diag = LspDiagnostic {
            uri: "file:///test.rs".into(),
            range: LspRange::new(LspPosition::new(4, 10), LspPosition::new(4, 20)),
            message: "unused variable: `x`".into(),
            severity: DiagnosticSeverity::Warning,
            code: None,
            source: Some("rustc".into()),
        };
        let pretty = diag.pretty();
        assert_eq!(pretty, "WARN [5:11] unused variable: `x`");
    }

    #[test]
    fn test_diagnostic_report() {
        let diags = vec![
            LspDiagnostic {
                uri: "file:///test.rs".into(),
                range: LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 1)),
                message: "error one".into(),
                severity: DiagnosticSeverity::Error,
                code: None,
                source: None,
            },
            LspDiagnostic {
                uri: "file:///test.rs".into(),
                range: LspRange::new(LspPosition::new(1, 0), LspPosition::new(1, 1)),
                message: "warning".into(),
                severity: DiagnosticSeverity::Warning,
                code: None,
                source: None,
            },
        ];
        let report = LspDiagnostic::report("test.rs", &diags);
        assert!(report.contains("ERROR"));
        assert!(report.contains("error one"));
        assert!(!report.contains("warning")); // Only errors in report
        assert!(report.contains("test.rs"));
    }

    #[test]
    fn test_diagnostic_report_no_errors() {
        let diags = vec![LspDiagnostic {
            uri: "file:///test.rs".into(),
            range: LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 1)),
            message: "just a warning".into(),
            severity: DiagnosticSeverity::Warning,
            code: None,
            source: None,
        }];
        let report = LspDiagnostic::report("test.rs", &diags);
        assert!(report.is_empty());
    }

    #[test]
    fn test_diagnostic_report_truncation() {
        // Create 25 errors — should be capped at 20 with "... and 5 more"
        let mut diags = Vec::new();
        for i in 0..25 {
            diags.push(LspDiagnostic {
                uri: "file:///test.rs".into(),
                range: LspRange::new(LspPosition::new(i, 0), LspPosition::new(i, 1)),
                message: format!("error {i}"),
                severity: DiagnosticSeverity::Error,
                code: None,
                source: None,
            });
        }
        let report = LspDiagnostic::report("test.rs", &diags);
        assert!(report.contains("... and 5 more"));
        let error_count = report.matches("ERROR").count();
        assert_eq!(error_count, 20); // capped at MAX_PER_FILE
    }

    #[test]
    fn test_diagnostic_default_severity_is_error() {
        let diag = LspDiagnostic {
            uri: "file:///test.rs".into(),
            range: LspRange::new(LspPosition::new(0, 0), LspPosition::new(0, 1)),
            message: "test".into(),
            severity: default_severity(),
            code: None,
            source: None,
        };
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
    }

    // -- SymbolKind ---------------------------------------------------------

    #[test]
    fn test_symbol_kind_values() {
        assert_eq!(SymbolKind::File as u32, 1);
        assert_eq!(SymbolKind::Module as u32, 2);
        assert_eq!(SymbolKind::Class as u32, 5);
        assert_eq!(SymbolKind::Function as u32, 12);
        assert_eq!(SymbolKind::TypeParameter as u32, 26);
    }

    #[test]
    fn test_interesting_kinds() {
        assert!(SymbolKind::Class.is_interesting());
        assert!(SymbolKind::Function.is_interesting());
        assert!(!SymbolKind::File.is_interesting());
        assert!(!SymbolKind::Module.is_interesting());
    }

    // -- Language extensions ------------------------------------------------

    #[test]
    fn test_language_id_for_extension() {
        assert_eq!(language_id_for_extension(".rs"), "rust");
        assert_eq!(language_id_for_extension(".ts"), "typescript");
        assert_eq!(language_id_for_extension(".tsx"), "typescriptreact");
        assert_eq!(language_id_for_extension(".py"), "python");
        assert_eq!(language_id_for_extension(".go"), "go");
        assert_eq!(language_id_for_extension(".zig"), "zig");
        // Unknown extension falls back to plaintext
        assert_eq!(language_id_for_extension(".unknown"), "plaintext");
    }

    // -- LspEvent -----------------------------------------------------------

    #[test]
    fn test_lsp_event_updated_serialization() {
        let event = LspEvent::Updated {};
        let json = serde_json::to_string(&event).expect("serialize");
        let expected = r#"{"type":"lsp.updated"}"#;
        assert_eq!(json, expected);
    }

    #[test]
    fn test_lsp_event_const() {
        assert_eq!(LspEvent::UPDATED, "lsp.updated");
    }

    // -- LspStatus ----------------------------------------------------------

    #[test]
    fn test_lsp_status_serialization() {
        let status = LspStatus {
            id: "rust".into(),
            name: "rust-analyzer".into(),
            root: "src/".into(),
            status: LspConnectionStatus::Connected,
        };
        let json = serde_json::to_string(&status).expect("serialize");
        let deserialized: LspStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.id, "rust");
        assert_eq!(deserialized.status, LspConnectionStatus::Connected);
    }

    // -- InitializeError ----------------------------------------------------

    #[test]
    fn test_initialize_error_display() {
        let err = InitializeError {
            server_id: "typescript".into(),
        };
        assert!(err.to_string().contains("typescript"));
    }

    // -- LspLocInput --------------------------------------------------------

    #[test]
    fn test_lsp_loc_input() {
        let input = LspLocInput {
            file: "/tmp/test.rs".into(),
            line: 42,
            character: 7,
        };
        assert_eq!(input.file, "/tmp/test.rs");
        assert_eq!(input.line, 42);
        assert_eq!(input.character, 7);
    }
}
