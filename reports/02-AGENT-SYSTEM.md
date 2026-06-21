# Agent System — Gap Analysis

## Architecture

| Aspect | TS | Rust |
|--------|----|------|
| Layers | V1 + V2 agent services, subagent permissions, prompt templates | Monolithic `agent.rs` (~1300 lines) |
| Prompt templates | 5 `.txt` files | All 5 as `pub const &str` |
| Built-in agents | 7 (build, plan, general, explore, compaction, title, summary) | Same 7 |

## Feature Gap Table

| Feature | TS | Rust | Severity |
|---------|----|------|----------|
| Prompt templates | 5 files | All ported as `pub const` | ✅ PARITY |
| AgentMode enum | Full | Full | ✅ PARITY |
| AgentInfo struct | 15 fields | 15 fields | ✅ PARITY |
| Subagent permission derivation | Full | Full | ✅ PARITY |
| Built-in agents | 7 | Same 7 | ✅ PARITY |
| Config merging | Full | Full | ✅ PARITY |
| Default agent resolution | Full | Full | ✅ PARITY |
| **V2 AgentV2.Info (branded ID, system, request)** | Full | **Missing** | **CRITICAL** |
| **Agent generation (LLM call)** | Full `generateObject` | **Stub** — NotImplemented | **CRITICAL** |
| **V2 agentic loop** (llm.ts) | Full V2 runner | V1-style `run_loop` only | **CRITICAL** |
| **LLM event publishing** | 15+ event types | Basic `Vec<LlmEvent>` | **CRITICAL** |
| **V2 runner model resolution** | Full catalog-aware | **Missing** | **CRITICAL** |
| V2 AgentV2.Service (select, resolve) | Full | **Missing** | **HIGH** |
| V2 Color type (hex + theme names) | Full | **Missing** | **HIGH** |
| Agent generation — OpenAI OAuth | Full | **Missing** | **HIGH** |
| Agent generation — Plugin hook | Full | **Missing** | **HIGH** |
| Context Epoch / AgentMismatch | Full | **Missing** | **HIGH** |
| SessionInput delivery (steer/queue) | Full | **Missing** | **HIGH** |
| Tool materialization with permissions | Full | **Missing** | **HIGH** |
| Overflow compaction recovery | Full | Heuristic only | **HIGH** |
| Tool fiber management | Full (FiberSet) | Sequential only | **HIGH** |
| System context assembly | Full | **Missing** | **HIGH** |
| Step limit error | Full | string only | **MEDIUM** |
| Question rejection | Full | **Missing** | **MEDIUM** |
| Doom-loop detection | Present | Present | LOW |
| Context overflow detection | Full | Weaker (string matching) | LOW |

## 5 Most Critical Gaps

### 1. V2 Agent Service (`core/agent.ts:11-142`)
Branded `ID`, `Color`, `Info` Schema.Class, `Selection`, `Service` — the V2 runner depends on this.

**TS**: `packages/core/src/agent.ts`
**Rust**: **MISSING**

### 2. Agent Generation (`packages/opencode/src/agent/agent.ts:366-434`)
Creates new agents via LLM call with provider pipeline, auth, plugin hook, schema validation.

**Rust**: Returns `Error::NotImplemented`

### 3. V2 Agentic Loop (`packages/core/src/session/runner/llm.ts:86-401`)
Full turn orchestration with context epoch, input delivery, overflow compaction, tool fibers.

**Rust**: V1-style loop only.

### 4. LLM Event Publishing (`publish-llm-event.ts:1-411`)
Translates raw LLMEvent stream into durable SessionEvent publications (15+ event types).

**Rust**: **MISSING**

### 5. V2 Runner Model Resolution (`runner/model.ts:42-166`)
Resolves session model selection into concrete `Model` with auth, endpoint, API-specific routing.

**Rust**: **MISSING**
