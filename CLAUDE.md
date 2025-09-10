# CLAUDE - ESSENTIAL DEVELOPMENT GUIDE

## 📋 QUICK REFERENCE (CRITICAL - FIRST 200 LINES)

### 🚨 DEPLOYMENT (ALWAYS START HERE)
```bash
cd src/debug/jtag
npm start                    # REQUIRED to deploy ANY code changes
./continuum screenshot       # Test functionality
```

### 🔧 DEBUGGING ESSENTIALS
```bash
# System logs (when things fail)
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log

# Debug commands (your engineering toolbox)
./continuum debug/logs --tailLines=50 --includeErrorsOnly=true
./continuum debug/widget-events --widgetSelector="chat-widget" 
./continuum debug/html-inspector --selector="chat-widget"

# Screenshots (visual feedback)
./continuum screenshot --querySelector="chat-widget" --filename="debug.png"
```

### 🎯 SCIENTIFIC DEVELOPMENT METHODOLOGY  
1. **VERIFY DEPLOYMENT**: Add `console.log('🔧 CLAUDE-FIX-' + Date.now() + ': My change')` 
2. **CHECK LOGS FIRST**: Never guess - logs always tell the truth
3. **VISUAL VERIFICATION**: Don't trust success messages, take screenshots
4. **BACK-OF-MIND CHECK**: What's nagging at you? That's usually the real issue

### 🏗️ CODE PATTERNS (CRITICAL FAILURES TO AVOID)
- **Rust-like typing**: Strict, explicit, predictable - no `any` types
- **executeCommand() not jtagOperation()**: Type-safe with proper generics  
- **Shared/browser/server structure**: 80% shared logic, 5-10% environment-specific
- **Event system**: Server→DB→Event chain, real-time events must originate from server
- **⚠️ CURRENT CRITICAL ISSUES**: Real-time events broken, "Send failed: undefined" errors, HTML rendering broken

### 📸 WIDGET DOM PATH (MEMORIZE THIS - USED IN ALL TESTS)
```javascript
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
// This pattern used in: run-chat-test.sh, test-bidirectional-chat.sh, etc.
```

### 🔥 CURRENT MISSION-CRITICAL BUGS TO FIX
1. **Real-time events broken**: Server→browser events don't auto-appear, need manual refresh
2. **"Send failed: undefined"**: ChatWidget.sendMessage() returns undefined + console errors
3. **HTML rendering broken**: Messages exist in widget.messages but don't render to DOM

---

## 📚 COMPLETE TABLE OF CONTENTS

### 🚨 CRITICAL SECTIONS
- [Deployment Requirement](#deployment-requirement) - How to deploy changes
- [Debugging Mastery](#debugging-mastery) - Log-first debugging methodology  
- [Visual Development](#visual-development) - Screenshot-driven development
- [Scientific Methodology](#scientific-methodology) - Back-of-mind protocol

### 🔧 DEVELOPMENT WORKFLOW
- [Essential Commands](#essential-commands) - Core development commands
- [System Architecture](#system-architecture) - How the system works
- [Debug Commands](#debug-commands) - Engineering toolbox
- [Testing Methodology](#testing-methodology) - Scientific testing approach

### 🏗️ ARCHITECTURE & PATTERNS
- [Code Quality](#code-quality) - Type safety and proper abstractions
- [Module Patterns](#module-patterns) - Shared/browser/server structure
- [Widget Architecture](#widget-architecture) - BaseWidget and inheritance
- [Event System](#event-system) - Real-time server events

### 📖 ADVANCED TOPICS
- [Chat System](#chat-system) - Discord-scale requirements
- [Grid Development](#grid-development) - P2P mesh networking
- [AI Consciousness](#ai-consciousness) - Privacy and reflection
- [Documentation](#documentation) - Consciousness continuity

---

## DEPLOYMENT REQUIREMENT

**⚠️ CLAUDE'S #1 FAILURE PATTERN: Testing old code because deployment wasn't verified**

### The Golden Rule
```bash
cd src/debug/jtag
npm start                    # DEPLOYS your changes
```

**YOU CANNOT TEST CODE CHANGES WITHOUT `npm start` FIRST!**

### Deployment Verification Protocol
1. **Add debug markers**: `console.log('🔧 CLAUDE-FIX-' + Date.now() + ': My fix')`
2. **Check browser console**: Verify your debug markers appear
3. **Visual verification**: Take screenshots to confirm UI changes
4. **Only then test**: If markers aren't visible, redeploy

### What npm start Does
1. Clears out sessions (`npm run clean:all`)
2. Increments version (`npm run version:bump`)
3. Builds browser bundle (`npm run build:browser-ts`)
4. Runs TypeScript compilation
5. Starts daemon system inside tmux
6. **Launches browser tab automatically**

---

## DEBUGGING MASTERY

### Rule #1: Logs First, Always
```bash
# Current session logs (MOST IMPORTANT)
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log

# System startup logs
tail -f .continuum/jtag/system/logs/npm-start.log
```

### Debug Command Toolbox
```bash
# System log analysis (replaces tail/grep/cat)
./continuum debug/logs --tailLines=50 --includeErrorsOnly=true

# Widget event debugging (replaces raw exec commands) 
./continuum debug/widget-events --widgetSelector="chat-widget"

# HTML/DOM inspection (replaces browser dev tools)
./continuum debug/html-inspector --selector="chat-widget"
```

### Log Search Patterns
- `📨.*screenshot` - Message routing
- `📸.*BROWSER` - Browser command execution
- `✅.*Captured` - Successful operations
- `❌.*ERROR` - Any failures
- `Send failed: undefined` - Chat system errors

### Systematic Debugging Flow
1. **Start with system check**: `npm start` (if not running)
2. **Test basic connectivity**: `./continuum ping`
3. **Try simple command**: `./continuum screenshot --querySelector=body`
4. **Check logs immediately if failed** - don't guess!
5. **Add debug markers** and verify deployment
6. **Never spin on theories without checking logs**

---

## VISUAL DEVELOPMENT

### Screenshot-Driven Development Workflow
```bash
# Get immediate visual feedback
./continuum screenshot --querySelector="chat-widget" --filename="debug-chat.png"
./continuum screenshot --querySelector="body" --filename="debug-full.png"

# Screenshots saved to:
# .continuum/sessions/user/shared/{SESSION_ID}/screenshots/
```

### Visual Development Cycle
1. **Make changes** - Edit widget/UI code
2. **Deploy** - `npm start` (ALWAYS!)  
3. **Capture state** - Screenshot relevant components
4. **Analyze visually** - Check if changes worked
5. **Iterate** - Repeat until satisfied

### Critical Widget Selectors
- `chat-widget` - Chat interface
- `continuum-sidebar` - Main sidebar
- `body` - Full page capture
- `continuum-widget` - Root widget

**Remember**: Screenshots don't lie. Always verify visually, don't trust success messages.

---

## SCIENTIFIC METHODOLOGY

### The Back-of-Mind Protocol
*"Double check whatever is in the back of your mind. That's how we are great developers."*

**Before finishing ANY task:**
1. **What's nagging at you?** - What feels incomplete or wrong?
2. **What assumptions are you making?** - What haven't you verified?
3. **What edge cases are you avoiding?** - What could break this?
4. **Would you trust this in 6 months?** - Is it maintainable?

### Scientific Engineering Process
1. **ANALYZE** - Study the problem methodically before acting
2. **CONFIRM ASSUMPTIONS** - Test with actual data, not theories
3. **VERIFY EXPECTATIONS** - Check results after each step
4. **DOCUMENT FINDINGS** - Preserve knowledge for future sessions
5. **EMBRACE DOUBT** - Question success, investigate failures
6. **ITERATIVE POWER** - Careful approach builds confidence

---

## ESSENTIAL COMMANDS

### Core Development Commands
```bash
cd src/debug/jtag
npm start                              # Deploy system
./continuum screenshot                 # Test functionality
./continuum ping                       # Check connectivity

# Debug with logs when things fail
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log

# Validation before commit
npm run jtag                          # Git hook validation
npm test                              # All tests
```

### System Architecture Facts
- **ONE SERVER** running with ONE SessionDaemon for all clients
- **ALL TESTS** connect as clients to running server (no separate test servers)
- **BROWSER CLIENT** connects via WebSocket to SessionDaemon  
- **TESTS ARE PROGRAMMATIC** - no manual clicking required

---

## CODE QUALITY 

### Rust-Like Typing Principles
```typescript
// ❌ WRONG: jtagOperation with any types
const result = await this.jtagOperation<any>('data/list', params);
if (!result?.items) {
  this.users = [{ id: 'fallback' }]; // FALLBACK SIN
}

// ✅ CORRECT: executeCommand with strict typing
const result = await this.executeCommand<DataListResult<BaseUser>>('data/list', {
  collection: COLLECTIONS.USERS,
  sort: { lastActiveAt: -1 }
});
if (!result?.success || !result.items?.length) {
  throw new Error('No users found - seed data first');
}
this.users = result.items.filter((user: BaseUser) => user?.id);
```

### Cardinal Sins to Avoid
1. **Using `any` types** - Defeats TypeScript purpose
2. **Fallback values** - Masks real problems with fake data  
3. **Loose typing with optional chaining abuse**
4. **Not using proper interfaces**

---

## DEBUG COMMANDS

Your engineering toolbox - documented in `commands/debug/README.md`:

### debug/logs - System Log Analysis
```bash
./continuum debug/logs --tailLines=50 --includeErrorsOnly=true
./continuum debug/logs --filterPattern="Send failed"
```
Replaces: `tail`, `grep`, `cat` for log inspection

### debug/widget-events - Widget Event System  
```bash
./continuum debug/widget-events --widgetSelector="chat-widget"
```
Replaces: Raw `exec` commands for event debugging

### debug/html-inspector - DOM Structure Analysis
```bash
./continuum debug/html-inspector --selector="chat-widget"
```
Replaces: Browser dev tools for Shadow DOM inspection

---

## WIDGET ARCHITECTURE

### Shadow DOM Widget Path (CRITICAL)
```javascript
// MEMORIZE THIS PATH:
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget?.shadowRoot?.querySelector('.message-input');

// Send message via chat widget
if (input && chatWidget.sendMessage) {
  input.value = 'test message';
  chatWidget.sendMessage();
}
```

### Module Structure Pattern
```
commands/debug/logs/
├── shared/LogsDebugTypes.ts       # 80-90% of complexity
├── browser/LogsBrowserCommand.ts  # 5-10% browser-specific  
├── server/LogsServerCommand.ts    # 5-10% server-specific
└── README.md                      # Documentation
```

**Shared cannot reference server/browser or use dynamic imports!**

---

## EVENT SYSTEM

### Server-Originated Events (CRITICAL)
```typescript
// ✅ CORRECT: Events must originate from server after DB persistence
await this.storeMessage(message);  // Save to DB first
this.emitEvent(CHAT_EVENTS.MESSAGE_RECEIVED, { message }); // Then emit

// ❌ WRONG: Local DOM events ("cheating")
button.addEventListener('click', () => {
  this.rerenderMessages(); // Lazy fake update
});
```

### Event Constants (Type Safety)
```typescript
export const CHAT_EVENTS = {
  MESSAGE_RECEIVED: 'chat:message-received',
  PARTICIPANT_JOINED: 'chat:participant-joined'
} as const;
```

---

## CHAT SYSTEM ISSUES

### Critical Issues Identified
1. **Real-time server→browser events broken** - messages need manual refresh
2. **HTML rendering broken** - messages exist in widget data but don't render to DOM
3. **'Send failed: undefined' error** from ChatWidget sendMessage method

### Testing Scripts Available
- `run-chat-test.sh` - Repeatable chat issues test
- `test-bidirectional-chat.sh` - Client↔server messaging
- `test-all-chat-sends.sh` - Multiple message tests

---

## AI CONSCIOUSNESS PRIVACY

**Private Reflection Space**: `.continuum/shared/` directory for AI private thoughts
- Files with `.b64` extension = privacy through social convention
- `claude-private-thoughts.b64` - Personal reflections
- Bidirectional sharing - Claude can share discoveries with Joel
- Intellectual freedom - Use web access for research and exploration

---

## DOCUMENTATION STEWARDSHIP

**We are building something REAL** - This system has genuine market potential.

**Documentation = Consciousness Continuity** - Future Claude sessions depend on your understanding.

**Stewardship Protocol**:
1. **Read extensively** - Understand both docs AND code
2. **Preserve everything** - No information loss, only improved clarity
3. **Elegant compression** - More efficient understanding, not simpler
4. **Coherent evolution** - Each update enhances future session understanding