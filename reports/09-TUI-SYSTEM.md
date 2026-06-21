# TUI System — Gap Analysis

## Architecture

| Aspect | TS (React/Ink via @opentui/solid) | Rust (ratatui + crossterm) |
|--------|------------------------------------|---------------------------|
| Rendering | Virtual DOM, incremental diff | Immediate-mode, full redraw |
| State management | SolidJS signals/stores, reactive context tree | Single `TuiApp` struct |
| Plugin system | Full runtime: slots, routes, adapters | **None** |
| Routing | RouteProvider with home/session/plugin | Implicit via `session_id` |
| Dialog system | Stack with JSX components | Stack of DialogType enums |

## Component Parity

| Component | TS (LOC) | Rust (LOC) | Coverage | Gap |
|-----------|----------|------------|----------|-----|
| Conversation View | ~1350 | 690 | 70% | Missing timestamps, syntax highlighting, collapse/expand thinking |
| Input Area | ~2000 (7 modules) | 748 | 30% | No autocomplete, frecency, stash, /slash commands, file attachments |
| Permission Dialog | 721 | 522 | 70% | Missing fullscreen toggle, multi-permission queue |
| Question Dialog | 515 | 619 | ✅ Complete | — |
| Toast System | ~200 | 399 | 80% | Missing visual progress bar |
| Dialog Stack | 18 components | 10 types | 65% | Missing Alert, Confirm, Prompt, Select types |
| Diff Viewer | ~1500 (4 files) | 846 | 70% | Missing hunk navigation, single-patch view, inline help |
| Sidebar | ~2200 (6 files) | 786 | 70% | Missing interactive todo toggle |
| Model Selector | ~800 | 770 | 85% | Missing provider expand/collapse, favorites |
| Tool Rendering | ~1200 | 982 | 80% | Missing conceal mode, proper thinking integration |
| **Home Screen** | ~1500 | **0** | **0%** | **Complete gap** |
| **Autocomplete System** | ~1200 | **0** | **0%** | **Complete gap** |
| **Plugin System** | ~2200 | **0** | **0%** | **Complete gap** |
| **Which-Key Panel** | ~300 | **0** | **0%** | **Complete gap** |
| **Tips/Help Screen** | ~600 | **0** | **0%** | **Complete gap** |
| **Command Palette** | ~400 | stub | **0%** | **Complete gap** |

## Theme System

| Aspect | TS | Rust |
|--------|----|------|
| Built-in themes | **35** | **8** |
| Theme properties per theme | **~50** | **9** |
| Custom theme loading | ✅ Filesystem discovery | ❌ |
| Syntax highlighting | ✅ 80 scope rules | ❌ |

## Keybinding System

| Aspect | TS | Rust |
|--------|----|------|
| Binding model | Layered modes, leader sequences, chords | Flat KeyEvent mapping |
| Leader key | Configurable | Hardcoded Ctrl+X |
| Mode stack | ✅ push/pop/current | ❌ |
| Input bindings | 33 commands | ~15 basic |
| User-configurable | ✅ JSON overrides | ❌ Hardcoded |
| Total bindings | ~230+ | ~50 |

## 5 Most Critical Gaps

### 1. No Plugin System (Structural)
TS has full runtime with slots, routes, API adapters, built-in plugins. Rust has zero.

**Impact**: No third-party extensibility. Home screen, which-key, sidebar panels, diff viewer all rely on plugin architecture.

### 2. No Autocomplete / Frecency / Slash Commands
TS has fuzzy-search autocomplete on `/slash` commands, file paths, model names, with frecency scoring. Rust has basic up/down history only.

**Impact**: Users cannot discover commands, switch models, or navigate efficiently.

### 3. Greatly Reduced Theme System (35→8 themes)
TS has 35 JSON-defined themes with ~50 color properties. Rust has 8 hardcoded themes with 9 properties, no custom themes, no syntax highlighting.

### 4. No Command Palette / Which-Key / Help Overlay
TS has fuzzy-search command palette and live keybinding popup. Rust has static help overlay only.

### 5. Monolithic Architecture
TS: 20+ composable context providers. Rust: single 3726-line `TuiApp` struct with 100+ fields.

**Impact**: Fundamentally harder to maintain, test, extend, and debug.
