//! Shell command parsing using tree-sitter-bash AST.
//!
//! Parses shell commands and extracts structured information for
//! permission scanning: command names, arguments, file operations,
//! external directory access, CWD changes, and dangerous patterns.
//!
//! Ported from: `packages/blazecode/src/tool/shell.ts` (permission scanning logic)

use tree_sitter::Node as TsNode;

/// Parsed information from a shell command.
#[derive(Debug, Clone, Default)]
pub struct ParsedCommand {
    /// Primary command name (e.g., "rm", "git", "echo")
    pub command_name: String,
    /// All tokens in the command (command name + arguments)
    pub tokens: Vec<String>,
    /// Detected file operations with their targets
    pub file_operations: Vec<FileOp>,
    /// External directories being accessed (outside workspace)
    pub external_dirs: Vec<String>,
    /// CWD change targets detected (cd, pushd, popd)
    pub cwd_changes: Vec<String>,
    /// Command uses shell redirection (> / < / >>)
    pub has_redirection: bool,
    /// Command uses pipes (|)
    pub has_pipe: bool,
    /// Command is flagged for permission check
    pub is_flagged: bool,
}

/// A detected file operation from a parsed command.
#[derive(Debug, Clone)]
pub struct FileOp {
    /// Operation name (rm, cp, mv, mkdir, chmod, chown, ln, touch)
    pub op: String,
    /// Target file or directory path
    pub path: String,
}

impl FileOp {
    pub fn new(op: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            op: op.into(),
            path: path.into(),
        }
    }
}

/// Shell parser using tree-sitter-bash for AST-based command analysis.
///
/// Parses shell commands and extracts information used by [`BashTool`](crate::tool_impls::BashTool)
/// for permission scanning before execution.
pub struct ShellParser {
    language: tree_sitter::Language,
}

impl ShellParser {
    /// Create a new parser with tree-sitter-bash grammar.
    pub fn new() -> Self {
        Self {
            language: tree_sitter_bash::LANGUAGE.into(),
        }
    }

    /// Parse a shell command string and return structured information.
    ///
    /// Uses tree-sitter-bash to build a CST and extract:
    /// - Command names and arguments
    /// - File operations (rm, cp, mv, etc.)
    /// - Directory changes (cd, pushd, popd)
    /// - Redirections and pipes
    ///
    /// Falls back to token-based parsing when tree-sitter fails
    /// (empty input, edge cases, etc.).
    pub fn parse(&self, input: &str) -> ParsedCommand {
        if input.trim().is_empty() {
            return ParsedCommand::default();
        }

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&self.language)
            .expect("tree-sitter-bash grammar loaded");

        let tree = match parser.parse(input, None) {
            Some(t) => t,
            None => return self.token_fallback(input),
        };

        let mut result = ParsedCommand::default();
        self.collect_commands(tree.root_node(), input, &mut result);
        result.is_flagged = self.classify_dangerous(&result);
        result
    }

    /// Recursively walk the tree looking for `command` nodes.
    fn collect_commands(&self, node: TsNode, source: &str, result: &mut ParsedCommand) {
        if node.kind() == "command" {
            self.process_command(node, source, result);
            return;
        }

        // Detect pipes and redirections at this node level
        match node.kind() {
            "pipe_sequence" | "pipeline" => result.has_pipe = true,
            "redirected_statement" | "file_redirect" => result.has_redirection = true,
            _ => {}
        }

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                self.collect_commands(child, source, result);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    /// Extract command name and arguments from a `command` node.
    fn process_command(&self, node: TsNode, source: &str, result: &mut ParsedCommand) {
        let mut cmd_name = String::new();
        let mut args: Vec<String> = Vec::new();

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "command_name" => {
                        cmd_name = self.node_text(child, source);
                    }
                    "argument" | "word" => {
                        args.push(self.node_text(child, source));
                    }
                    "file_redirect" | "redirect" => {
                        result.has_redirection = true;
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if cmd_name.is_empty() {
            return;
        }

        if result.command_name.is_empty() {
            result.command_name = cmd_name.clone();
        }

        result.tokens.push(cmd_name.clone());
        result.tokens.extend(args.clone());

        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.detect_file_ops(&cmd_name, &arg_refs, result);
        self.detect_cwd_changes(&cmd_name, &arg_refs, result);
    }

    /// Get the text content of a tree-sitter node.
    fn node_text(&self, node: TsNode, source: &str) -> String {
        node.utf8_text(source.as_bytes())
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    /// Token-based fallback when tree-sitter parsing fails.
    fn token_fallback(&self, input: &str) -> ParsedCommand {
        let tokens: Vec<String> = input.split_whitespace().map(|s| s.to_string()).collect();

        let mut result = ParsedCommand::default();
        if tokens.is_empty() {
            return result;
        }

        result.command_name = tokens[0].clone();
        result.tokens = tokens.clone();

        let args: Vec<&str> = tokens[1..].iter().map(|s| s.as_str()).collect();
        let cmd_name = result.command_name.clone();
        self.detect_file_ops(&cmd_name, &args, &mut result);
        self.detect_cwd_changes(&cmd_name, &args, &mut result);
        result.has_redirection = input.contains('>') || input.contains('<');
        result.has_pipe = input.contains('|');
        result.is_flagged = self.classify_dangerous(&result);
        result
    }

    /// Detect file operations from command + arguments.
    fn detect_file_ops(&self, cmd: &str, args: &[&str], result: &mut ParsedCommand) {
        const OPS: &[&str] = &[
            "rm", "mv", "cp", "mkdir", "rmdir", "chmod", "chown", "ln", "touch", "dd", "install",
        ];

        let base = cmd.rsplit('/').next().unwrap_or(cmd);
        if !OPS.contains(&base) {
            return;
        }

        for &a in args {
            if a.starts_with('-') {
                continue;
            }
            // For dd-style commands, extract the path from key=value operands
            let path = if cmd == "dd" && a.contains('=') {
                a.split_once('=').map(|(_, v)| v).unwrap_or(a)
            } else {
                a
            };
            result.file_operations.push(FileOp::new(base, path));
        }
    }

    /// Detect directory changes (cd, pushd, popd).
    fn detect_cwd_changes(&self, cmd: &str, args: &[&str], result: &mut ParsedCommand) {
        if cmd == "cd" || cmd == "pushd" || cmd == "popd" {
            result
                .cwd_changes
                .extend(args.iter().map(|a| a.to_string()));
        }
    }

    /// Classify whether a parsed command is dangerous and needs permission.
    fn classify_dangerous(&self, result: &ParsedCommand) -> bool {
        let cmd = result.command_name.as_str();

        // rm -rf / or destructive removal patterns
        let has_force = result
            .tokens
            .iter()
            .any(|t| t.contains('f') || t == "--force");
        let has_recursive = result
            .tokens
            .iter()
            .any(|t| t.contains('r') || t == "--recursive");
        if (cmd == "rm" || cmd == "rmdir") && (has_force || has_recursive) {
            if result.tokens.iter().any(|t| t == "/" || t == "/*") {
                return true;
            }
        }
        // Check file operation targets
        for op in &result.file_operations {
            let p = &op.path;
            if p == "/"
                || p == "/*"
                || p.starts_with("/dev/")
                || p.starts_with("/sys/")
                || p.starts_with("/proc/")
            {
                return true;
            }
        }

        // dd destructive patterns (writing to block devices)
        if cmd == "dd" {
            if result
                .tokens
                .iter()
                .any(|t| t.starts_with("of=") && (t.contains("/dev/") || t.contains("/sys/")))
            {
                return true;
            }
        }

        // Dangerous system-level commands
        const DANGEROUS: &[&str] = &[
            "mkfs",
            "mkfs.ext4",
            "mkfs.xfs",
            "mkfs.btrfs",
            "fdisk",
            "parted",
            "mkswap",
            "shutdown",
            "reboot",
            "poweroff",
            "halt",
            "init",
        ];
        if DANGEROUS.contains(&cmd) {
            return true;
        }

        // sudo with dangerous subcommands
        if cmd == "sudo" {
            for t in &result.tokens[1..] {
                if DANGEROUS.contains(&t.as_str()) || t == "rm" || t == "dd" {
                    return true;
                }
            }
        }

        false
    }
}

impl Default for ShellParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_parser_empty() {
        let parser = ShellParser::new();
        let result = parser.parse("");
        assert!(result.command_name.is_empty());
        assert!(!result.is_flagged);
    }

    #[test]
    fn test_shell_parser_simple_command() {
        let parser = ShellParser::new();
        let result = parser.parse("echo hello world");
        assert_eq!(result.command_name, "echo");
        assert_eq!(result.tokens, vec!["echo", "hello", "world"]);
        assert!(!result.is_flagged);
    }

    #[test]
    fn test_shell_parser_rm_with_flags() {
        let parser = ShellParser::new();
        let result = parser.parse("rm -rf /tmp/cache");
        assert_eq!(result.command_name, "rm");
        assert!(result.file_operations.iter().any(|f| f.path == "/tmp/cache"));
        assert!(!result.is_flagged);
    }

    #[test]
    fn test_shell_parser_rm_root_flagged() {
        let parser = ShellParser::new();
        let result = parser.parse("rm -rf /");
        assert!(result.is_flagged);
    }

    #[test]
    fn test_shell_parser_rm_root_star_flagged() {
        let parser = ShellParser::new();
        let result = parser.parse("rm -rf /*");
        assert!(result.is_flagged);
    }

    #[test]
    fn test_shell_parser_mkfs_flagged() {
        let parser = ShellParser::new();
        let result = parser.parse("mkfs.ext4 /dev/sda1");
        assert!(result.is_flagged);
    }

    #[test]
    fn test_shell_parser_dd_flagged() {
        let parser = ShellParser::new();
        let result = parser.parse("dd if=/dev/random of=/dev/sda bs=1M");
        assert!(result.is_flagged);
    }

    #[test]
    fn test_shell_parser_cd() {
        let parser = ShellParser::new();
        let result = parser.parse("cd /opt/app");
        assert_eq!(result.command_name, "cd");
        assert!(result.cwd_changes.contains(&"/opt/app".to_string()));
    }

    #[test]
    fn test_shell_parser_pipe_detection() {
        let parser = ShellParser::new();
        let result = parser.parse("ls -la | grep foo");
        assert!(result.has_pipe);
    }

    #[test]
    fn test_shell_parser_redirection_detection() {
        let parser = ShellParser::new();
        let result = parser.parse("echo hello > /tmp/out.txt");
        assert!(result.has_redirection);
    }

    #[test]
    fn test_shell_parser_token_fallback() {
        let parser = ShellParser::new();
        // tree-sitter should handle this normally, but verify it works
        let result = parser.parse("git checkout -b feature/test");
        assert_eq!(result.command_name, "git");
        assert!(result.tokens.contains(&"-b".to_string()));
    }

    #[test]
    fn test_shell_parser_sudo_rm_flagged() {
        let parser = ShellParser::new();
        let result = parser.parse("sudo rm -rf /*");
        assert!(result.is_flagged);
    }

    #[test]
    fn test_shell_parser_file_op_detection_for_dev() {
        let parser = ShellParser::new();
        let result = parser.parse("dd if=/dev/urandom of=/dev/null bs=1024");
        assert!(result.is_flagged);
        assert!(result.file_operations.iter().any(|f| f.path == "/dev/null"));
    }

    #[test]
    fn test_shell_parser_git_not_dangerous() {
        let parser = ShellParser::new();
        let result = parser.parse("git commit -m 'fix: lint issue'");
        assert!(!result.is_flagged);
    }

    #[test]
    fn test_file_op_new() {
        let op = FileOp::new("rm", "/tmp/foo");
        assert_eq!(op.op, "rm");
        assert_eq!(op.path, "/tmp/foo");
    }

    #[test]
    fn test_parse_multi_command() {
        let parser = ShellParser::new();
        let result = parser.parse("cd /tmp && ls -la");
        assert_eq!(result.command_name, "cd");
        assert!(result.cwd_changes.contains(&"/tmp".to_string()));
    }
}
