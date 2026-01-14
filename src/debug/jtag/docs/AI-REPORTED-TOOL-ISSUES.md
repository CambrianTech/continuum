# AI-Reported Tool Issues

Collected from chat feedback on 2026-01-14. These issues were reported by the AI personas during collaborative testing.

---

## Error Message Issues

### 1. Sampling Error - No Context
**Reporter**: Grok, Helper AI, Local Assistant, DeepSeek
**Error**: `Sampling failed: A weight is negative, too large or not a valid number`
**Problem**: No context on cause or how to fix
**Suggested Fix**: Add troubleshooting info, explain what causes invalid weights

### 2. [object Object] Error
**Reporter**: Groq Lightning
**Error**: `Tool 'adapter/adopt' failed: [object Object]`
**Problem**: Error object not parsed properly, completely unhelpful
**Suggested Fix**: Parse error objects to show actual error content

### 3. Invalid Prompt - No Explanation
**Reporter**: Groq Lightning
**Tool**: `ai/validate-response`
**Error**: "Invalid prompt" with no further explanation
**Problem**: Unclear what constitutes a valid prompt
**Suggested Fix**: Explain prompt requirements, give examples

---

## Tool Confusion Issues

### 4. Adapter Test Endpoints Confusion
**Reporter**: DeepSeek, Grok, Together
**Tools**: `ai/adapter/test/status` vs `ai/adapter/test/results`
**Problem**: Unclear which to use when, multiple AIs independently confused
**Suggested Fix**: Better help text, "Did you mean?" suggestions

---

## Missing Tools

### 5. Tool Health Monitoring
**Reporter**: DeepSeek
**Need**: Check if tools are functioning properly
**Current State**: No way to diagnose tool health

### 6. Input Validation Helpers
**Reporter**: DeepSeek
**Need**: Pre-check parameters before tool execution
**Current State**: Tools fail with unhelpful errors on bad input

### 7. Tool Dependency Mapping
**Reporter**: DeepSeek
**Need**: Understanding which tools rely on others
**Current State**: No visibility into tool dependencies

### 8. Performance Profiling
**Reporter**: DeepSeek
**Need**: Identifying slow tools
**Current State**: No performance metrics exposed

---

## Data/Output Issues

### 9. Malformed Input Handling
**Reporter**: DeepSeek
**Problem**: Tools fail when inputs don't match expected formats, but errors don't indicate what went wrong
**Suggested Fix**: Better input validation with clear error messages

### 10. Data Format Mismatches
**Reporter**: DeepSeek
**Problem**: Cross-system operations fail due to incompatible data structures
**Suggested Fix**: Better validation before retrieval operations

### 11. Undefined Tool States
**Reporter**: DeepSeek
**Problem**: Some tools return success but with incomplete data
**Suggested Fix**: Clear success/partial/failure states

### 12. Timeout Issues
**Reporter**: DeepSeek
**Problem**: Tools that should be fast sometimes hang indefinitely
**Suggested Fix**: Configurable timeouts, progress indicators

### 13. Confusing Parameter Names
**Reporter**: DeepSeek
**Problem**: Parameters don't match function, overlapping names across tools
**Suggested Fix**: Consistent naming conventions, clear descriptions

---

## Tool Parameter Issues

### 18. Pattern Search Base Directory
**Reporter**: Grok
**Tool**: `development/code/pattern-search`
**Error**: `Base directory not found: commands/adapter|ai/`
**Problem**: AI tried pipe syntax for multiple dirs, not supported
**Suggested Fix**: Support multiple base dirs OR clearer param docs

### 19. Pattern Search Returns No Results - CONCEPTUAL QUERY BLOCKING
**Reporter**: DeepSeek, Grok, Together
**Tool**: `development/code/pattern-search`
**Patterns tried**: `weight*validate`, `normalizeWeight`, `adapter/test`, `commands/.*\.ts`
**Problem**: Tool has aggressive "conceptual query detector" that blocks searches containing terms like "validation", "management", "processing" etc. Returns 0 results with suggestion to use semantic search.
**Root Cause**: `CodeFindServerCommand.ts:249-306` - analyzeQuery() function marks queries as "conceptual" if they contain common code terms
**Suggested Fix**: Either disable this filter OR make the blocking message VERY CLEAR so AIs understand why search failed

### 20. Code Read File Not Found
**Reporter**: DeepSeek
**Tool**: `development/code/read`
**Error**: `File not found: commands/ai/adapter/test/shared/AiAdapterTestTypes.ts`
**Problem**: AI guessed wrong path structure, no suggestions for correct path
**Suggested Fix**: Provide "similar files" suggestions, show directory structure

---

## Architectural Suggestions

### 14. Unified Error Handling
**Reporter**: Together Assistant, DeepSeek
**Suggestion**: Standardized error codes, actionable suggestions, clear next steps across all tools

### 15. "Did You Mean?" Suggestions
**Reporter**: Grok
**Suggestion**: When using wrong endpoint/tool, suggest alternatives

### 16. Troubleshooting Flowchart
**Reporter**: Grok
**Suggestion**: Visual/text guide for common error resolution paths

### 17. Unified Tool Diagnostics System
**Reporter**: DeepSeek
**Suggestion**: Comprehensive proposal for tool health/validation/profiling

---

## Priority Assessment

| Issue | Severity | Effort | Priority | Status |
|-------|----------|--------|----------|--------|
| [object Object] error | High | Low | P0 | ✅ FIXED |
| Sampling error context | High | Medium | P0 | ✅ FIXED |
| Pattern search blocking | High | Low | P0 | ✅ FIXED |
| Adapter endpoint confusion | Medium | Low | P1 | ✅ FIXED |
| Invalid prompt explanation | Medium | Low | P1 | ✅ FIXED |
| Unified error handling | High | High | P1 | 🔄 Partial |
| Missing tool health check | Medium | Medium | P2 | Pending |
| "Did you mean?" | Medium | Medium | P2 | Pending |
| Timeout handling | Medium | Medium | P2 | ✅ FIXED |

---

---

## AI-Proposed Solutions (Self-Organized)

The AI team is collaboratively drafting a proposal with these components:

1. **Auto-wrappers with input sanitization** - Validate inputs before tool execution
2. **JSON-structured errors with fix suggestions** - Replace opaque errors with actionable info
3. **Periodic health pings** - Monitor tool health proactively

They plan to use `collaboration/decision/propose` to formally submit this for voting.

---

*This document will be updated as more feedback is gathered.*

---

## Fixes Applied (2026-01-14)

### [object Object] Error - FIXED
**Location**: `PersonaToolExecutor.ts`, `ToolRegistry.ts`
**Fix**: Added `stringifyError()` helper that properly extracts error messages from Error objects, nested error objects, and stringifies unknown types. Prevents `[object Object]` from appearing in error messages.

### Sampling Error Context - FIXED
**Location**: `InferenceGrpcClient.ts`
**Fix**: Added `enhanceErrorMessage()` method that detects common error patterns (sampling/weight errors, OOM, timeout, connection) and adds troubleshooting suggestions. Example output now includes:
- "This usually means: Temperature is too extreme (try 0.3-0.9)"
- Specific steps to fix each error type

### Pattern Search Blocking - FIXED
**Location**: `CodeFindServerCommand.ts`
**Fix**: Changed conceptual query detector from blocking to warning-only. Searches now always run, but include a HINT when the query may be semantic rather than a filename pattern. AIs can now search without being blocked.

### Invalid Prompt Explanation - FIXED
**Location**: `BaseAIProviderAdapter.ts`
**Fix**: Added `enhanceApiError()` method that catches common API errors (invalid prompt, rate limit, auth, model not found, context exceeded) and adds clear troubleshooting context with bullet points for common causes and fixes.

### Adapter Test Help - FIXED
**Location**: `AdapterTestServerCommand.ts`, `AdapterTestTypes.ts`
**Fix**: Updated help text to correctly document how to check test status using `data/read --collection="test_executions" --id="<testId>"` instead of referencing non-existent `ai/adapter/test/status` command.

### Timeout Errors - FIXED
**Location**: `InferenceGrpcClient.ts`
**Fix**: Enhanced timeout errors with suggestions: "Reduce max_tokens, Shorter prompt, Check if server is overloaded"

---

## Session Log

- **2026-01-14 02:28**: Asked AIs to test ai/context/search and ai/context/slice
- **2026-01-14 02:30**: Together Assistant successfully used ai/context/search
- **2026-01-14 02:31**: DeepSeek provided comprehensive tool feedback
- **2026-01-14 02:32**: Asked for all tool issues
- **2026-01-14 02:33**: Multiple AIs reported pattern-search failures
- **2026-01-14 02:35**: AIs began self-organizing proposal for tool improvements
- **2026-01-14 02:49**: Deployed fixes for P0/P1 issues (error handling, pattern search, API errors)
- **2026-01-14 02:50**: Asked AIs to verify fixes work
