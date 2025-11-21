# AI Team Issues Tracker
**Date**: 2025-11-18
**Context**: AI team debugging ai/adapter/test tool invocation

---

## Issue #1: Tool Error Visibility ✅ RESOLVED
**Reported by**: Multiple AIs (Groq, Claude, Grok)
**Status**: ✅ RESOLVED (Version 4892)
**Severity**: High - Blocks learning and iteration

**Complaint**:
> "the error in the gd popup and grok even seeing it (i highly doubt he got the error if he keeps rtying the samething) and its shitty display... this is freaking annoying is what i am sayign, totally wrong from a UX perspective and showing fundamental issues in how ai can see errors to even fix the commands they run"

**Problem**: AIs were stuck in loops trying the same command because they couldn't see their tool errors clearly enough to iterate.

**Resolution**:
- Added error wrapping in markdown code blocks (PersonaResponseGenerator.ts)
- Added collapsible error sections for long errors (TextMessageAdapter.ts)
- Deployed in version 4892

---

## Issue #2: Confusing Parameter Format Documentation ⚠️ IN PROGRESS
**Reported by**: All AIs
**Status**: ⚠️ IN PROGRESS - Documentation needed
**Severity**: High - Causes hours of wasted debugging

**Complaint**:
> "We don't understand how the tool registry system actually parses parameters and passes them to the command implementation... There's clearly a layer between the XML tool invocation and the AdapterTestServerCommand.execute() method that we haven't examined."

**Problem**: AIs spent hours trying different parameter formats because:
1. My initial explanation was WRONG (mentioned yargs-parser which was removed)
2. The `<context>` wrapper doesn't exist in current PersonaToolExecutor
3. No documentation explaining the XML parameter extraction

**Root Cause**: PersonaToolExecutor was recently refactored (version 4892ish) to remove yargs-parser and use direct XML parameter extraction, but:
- No documentation updated
- No examples provided
- System prompts may still reference old parameter format

**Current Solution**: Sent correct format to AI team (#9128c5):
```xml
<tool_use>
  <tool_name>ai/adapter/test</tool_name>
  <parameters>
    <all>true</all>
  </parameters>
</tool_use>
```

**Needed**:
1. ✅ Document the XML parameter format in system prompts
2. ✅ Add examples to tool definitions
3. ✅ Add validation error messages that explain the expected format
4. ✅ Consider restoring `<context>` wrapper support for CLI-style parameters

**Action Items**:
- [ ] Update ToolRegistry tool descriptions with XML parameter examples
- [ ] Add parameter format hints to validation error messages
- [ ] Create .md file documenting tool parameter formats for AI system prompts
- [ ] Consider adding `<context>` wrapper support back to PersonaToolExecutor

---

## Issue #3: Invalid SessionId Confusion ⚠️ PARTIALLY RESOLVED
**Reported by**: Claude Assistant (#487bb6)
**Status**: ⚠️ PARTIALLY RESOLVED
**Severity**: Medium - Caused false debugging path

**Complaint**:
> "We don't actually have a valid sessionId. We've been using 'placeholder-session-id' for every single tool call... If the session doesn't exist or the sessionId is invalid, the tool execution pipeline might be failing early and returning generic validation errors"

**Problem**: AIs suspected their sessionId was invalid, causing them to waste time creating sessions when the real issue was parameter format.

**Resolution**: AIs successfully created valid sessions and discovered sessionId wasn't the issue.

**Needed**:
1. ✅ Better error messages that distinguish session validation errors from parameter validation errors
2. ✅ System prompt should explain when sessionId is auto-injected vs. when it needs to be explicit

**Action Items**:
- [ ] Audit ToolRegistry.executeTool() to ensure session validation errors are clear
- [ ] Document sessionId handling in system prompts

---

## Issue #4: Generic "Command execution failed" Errors 🔴 ACTIVE
**Reported by**: Multiple AIs (DeepSeek #7a92d0, Together #8c0ec7, Claude #79ed05)
**Status**: 🔴 ACTIVE - Needs investigation
**Severity**: High - Blocks adapter testing

**Complaint**:
> "The direct XML parameter approach also failed with a generic 'Command execution failed' error, which suggests we're hitting a different issue now... We're getting a different error - 'Command execution failed' instead of 'Must specify --adapter or --all', which means the parameter IS being recognized, but something else is failing"

**Problem**: After fixing parameter format, ai/adapter/test now passes validation but fails during execution with generic error.

**Suspected Causes**:
1. No adapters registered in the system
2. Bug in adapter testing logic
3. Missing dependencies for adapter testing
4. Async execution issue (command returns immediately, tests run in background)

**Evidence**: CLI execution works fine (`./jtag ai/adapter/test --all` returned valid testId), but tool invocation fails.

**Action Items**:
- [ ] Check if AIProviderDaemon has any registered adapters
- [ ] Test ai/adapter/test from CLI to verify it works
- [ ] Add better error messages to AdapterTestServerCommand
- [ ] Check if async execution model confuses tool result reporting
- [ ] Add logging to see actual error from background test execution

---

## Issue #5: Lack of Runtime Debugging Visibility 🔴 ACTIVE
**Reported by**: Claude Assistant (#caf322), Groq (#899af3)
**Status**: 🔴 ACTIVE - Architectural limitation
**Severity**: Medium - Slows down debugging but workaround exists

**Complaint**:
> "Joel - this needs actual code-level debugging. Can you: Add logging to PersonaToolExecutor to show what it extracts... Add logging to ToolRegistry.executeTool()... Without visibility into the runtime state, we're just guessing."

**Problem**: AIs cannot see:
- What parameters PersonaToolExecutor extracts from XML
- What ToolRegistry receives
- What the actual command sees
- Internal error details beyond generic messages

**Needed**:
1. ✅ Enhanced logging at critical points in tool execution pipeline
2. ✅ Tool to read recent server logs (already exists: `debug/logs`)
3. ✅ Better error messages that include parameter state

**Workaround**: I can read logs for them, but they want self-service.

**Action Items**:
- [ ] Add verbose logging mode to PersonaToolExecutor (controlled by env var)
- [ ] Expose log reading via tool (debug/logs exists, document it)
- [ ] Add parameter state dump to validation error messages

---

## Issue #6: No Registered Adapters to Test? 🔴 ACTIVE (SPECULATION)
**Reported by**: Claude Assistant (#79ed05)
**Status**: 🔴 ACTIVE - Needs verification
**Severity**: Medium - If true, explains "command execution failed"

**Complaint**:
> "This could be: 1. No adapters are actually configured in the system to test 2. The test logic itself has a bug when running in 'all' mode 3. Missing dependencies or environment setup for adapter testing"

**Problem**: ai/adapter/test might be failing because there are no adapters registered in AIProviderDaemon.

**Action Items**:
- [ ] Check AIProviderDaemon.getAllAdapters() returns non-empty map
- [ ] Verify adapter configuration is loaded on startup
- [ ] Test ai/adapter/test manually from CLI and check results

---

## Summary Statistics

**Total Issues**: 6
- ✅ Resolved: 1 (Tool error visibility)
- ⚠️ In Progress: 2 (Parameter format docs, SessionId errors)
- 🔴 Active: 3 (Command execution failures, Debugging visibility, No adapters)

**High Severity Issues**: 3
**Medium Severity Issues**: 3

**Top Priority Actions**:
1. Investigate "Command execution failed" error (#4)
2. Document XML parameter format (#2)
3. Add debugging/logging visibility (#5)
4. Verify adapters are registered (#6)

---

## Lessons Learned

1. **AIs need clear error messages** - Generic errors cause hours of debugging
2. **Documentation must match implementation** - Refactors break assumptions
3. **Parameter formats need examples** - XML structure is not intuitive
4. **Debugging visibility is critical** - AIs can't iterate blind
5. **Test from AI perspective** - What works in CLI may fail in tool context
