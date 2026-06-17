# Phase 1 Plan — Module [09] Tool

## Source Files Read

| File | Lines | Purpose |
|------|-------|---------|
| `packages/opencode/src/tool/tool.ts` | 183 | Core types, define(), wrap(), InvalidArgumentsError |
| `packages/opencode/src/tool/registry.ts` | 441 | ToolRegistry service, plugin tools, built-in tools list |
| `packages/opencode/src/tool/schema.ts` | 15 | ToolID brand schema |
| `packages/opencode/src/tool/read.ts` | 387 | Read file tool implementation |
| `packages/opencode/src/tool/write.ts` | 105 | Write file tool implementation |
| `packages/opencode/src/tool/shell.ts` | ~400+ | Bash/PTY tool implementation |

## Rust Design

### Changes to existing tool.rs

The existing 110-line stub needs expansion to ~800+ lines:

1. **ToolDef struct**: A concrete wrapper combining Tool trait + metadata (id, json_schema, format_validation_error)
2. **ExecuteResult**: Add attachments Vec<String> field
3. **ToolContext**: Add call_id, extra fields, metadata callback, ask callback for permission gating
4. **DynamicDescription**: Support for description that depends on agent context
5. **ToolInfo**: Lightweight metadata for deferred tool init
6. **ToolRegistry**: Thread-safe with DashMap, plugin tool support, tools_for_model filtering
7. **ToolOutput**: Streaming output support via channels
8. **ToolOutputTruncator**: Output truncation with limits
9. **InvalidArgumentsError**: Canonical "rewrite input" error
10. **PluginTool**: External/MCP tools registered at runtime

### Implementation Steps

Step A: Expand data structures (ToolDef, ExecuteResult, ToolInfo, PluginTool)
Step B: Expand ToolContext with ask/metadata callbacks, create ToolRegistry with DashMap
Step C: Add output truncation, streaming tool output support
Step D: Add InvalidArgumentsError pattern
Step E: Add plugin tool support
Step F: Tests
