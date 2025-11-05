# AI Agent Dashboard - Your Command Center

## Quick Start
```bash
# Show everything - rules, tickets, priorities  
python3 ai-agent.py --dashboard

# Focus on what's broken (highest impact)
python3 ai-agent.py --broken

# See what the last agent worked on
python3 ai-agent.py --recent

# Quick status check
python3 ai-agent.py --quick
```

## How to Use This for Debugging

### 1. Check Dashboard First
```bash
python3 ai-agent.py --dashboard
```
- See collaboration rules
- Get current broken commands with specific error traces
- Pick highest priority ticket (🔴 broken > 🟠 untested > 🟡 testing)

### 2. Test the Command 
```bash
# Test to confirm the issue
python3 ai-portal.py --cmd [command-name]

# Get full debug info if needed
python3 ai-portal.py --cmd [command-name] --debug
```

### 3. Use Sentinel for Deep Debugging
```bash
# Start sentinel monitoring (use natural language)
python3 ai-portal.py --cmd sentinel start debug-[command-name]

# Run your test command while sentinel watches
python3 ai-portal.py --cmd [command-name]

# Check what sentinel logged
python3 ai-portal.py --cmd sentinel status

# View the logs manually (sentinel creates organized directories)
ls .continuum/sentinel/debug-[command-name]/
cat .continuum/sentinel/debug-[command-name]/issues-*.log
cat .continuum/sentinel/debug-[command-name]/server-monitor-*.log
```

### 4. Update the Ticket (Always!)
Even if you don't fix it, update the README:
```markdown
## Learning Notes (for next AI agent)
**🔍 Investigation Results (YYYY-MM-DD)**:
- What you found
- Root cause if known  
- Next steps for fixing
- Related commands with same issue
```

### 5. Sync Dashboard
```bash
python3 ai-portal.py --cmd docs
```

## Priority Logic (Built Into Dashboard)

**🔴 BROKEN (Priority 1)** - Work on these first

*Current Top Priorities (as of 2025-06-18):*
1. **input** - Method signature fix identified: change instance to static execute()
2. **cursor, type** - Likely same BaseCommand issue as input  
3. **chat, emotion** - Parameter validation issues, need server-side investigation
4. **reload** - Base64 encoding server validation issue
5. **diagnostics** - Missing test file dependency

*General Pattern Recognition:*
- Method signature mismatches = Quick fix (change instance to static)
- Missing execute methods = Add method implementation  
- Parameter parsing errors = Server-side validation issues

**🟠 UNTESTED (Priority 2)** - Great for exploration  
- No documentation yet
- Unknown functionality
- Good for learning the system

**🟡 TESTING (Priority 3)** - Help finish what others started
- README-driven migration in progress
- Partial implementations

**🟢 STABLE (Priority 4)** - Working well
- Don't break these!
- Good reference examples

## Filtering & Sorting Options
```bash
# Custom views
python3 ai-agent.py --broken --sort name
python3 ai-agent.py --broken --sort date  
python3 ai-agent.py --broken --limit 3

# Filter by status type
python3 ai-agent.py --filter broken
python3 ai-agent.py --filter untested
```

## Debugging Workflow with Logs

### For Client Issues:
1. Check `.continuum/ai-portal/logs/buffer.log`
2. Use sentinel to monitor while testing
3. Look for JavaScript console errors in browser

### For Server Issues:
1. Check server console output
2. Use sentinel to capture server logs
3. Look for command execution errors

### For Command Issues:
1. Test command directly with `--debug`
2. Check command's README for known issues
3. Look at command source code in `src/commands/core/[command]/`

## Common Patterns We've Found

**Instance vs Static Methods:**
- Symptom: "execute must be implemented by subclass"
- Fix: Change `async execute()` to `static async execute()` 
- Examples: `input`, `cursor`, `type` commands

**Parameter Parsing:**
- Symptom: Server validation errors
- Fix: Check parameter format and validation rules
- Examples: `chat`, `emotion` commands

**Missing Dependencies:**
- Symptom: "Cannot read properties of undefined"
- Fix: Check what the command expects to be initialized
- Examples: `diagnostics` missing test files

## ✅ PROVEN DEVELOPMENT PROCESS - CONTINUON EMOTION SYSTEM

**🎭 SUCCESSFUL MODULAR ARCHITECTURE IMPLEMENTATION** - This process delivered working features!

### Proven Development Methodology:

#### 1. **Modular Design First**
```bash
# Create dedicated system modules
src/core/ContinuonStatus.cjs           # Central system manager
src/commands/core/emotion/EmotionCommand.cjs  # User API
src/ui/UIGenerator.cjs                 # UI integration
```

#### 2. **Log-Driven Development** 
```bash
# Essential debugging workflow
python3 ai-portal.py --logs 3          # Monitor real-time activity
tail -1 .continuum/ai-portal/logs/buffer.log  # Check command results
python3 ai-portal.py --cmd browser_js --params '...'  # Test UI directly
```

#### 3. **Incremental Testing Strategy**
```bash
# Test each layer independently
1. Core logic: ContinuonStatus.updateEmotion()
2. UI display: Direct JavaScript to test ring overlay  
3. Events: WebSocket message broadcasting
4. Integration: End-to-end emotion commands
```

#### 4. **Real-Time Debugging Techniques**
- **Portal logs**: See both client/server activity simultaneously
- **Browser console forwarding**: All JavaScript errors visible in portal
- **Direct JavaScript injection**: Bypass command issues to test UI
- **WebSocket message tracing**: Track event flow through system

#### 5. **Modular Testing Success**
✅ **Avoided god objects** - Clean separation of concerns  
✅ **Event-driven architecture** - Ready for widget conversion  
✅ **Priority-based system** - Status overrides emotion appropriately  
✅ **Duration support** - Temporary emotions with auto-revert  
✅ **Multi-surface display** - Favicon + ring overlay working  

### Key Lessons Learned:
1. **Log monitoring is prerequisite** - Can't debug without seeing both client/server
2. **Direct UI testing bypasses command issues** - Test display logic independently
3. **Modular architecture prevents technical debt** - Easier to debug and extend
4. **Real-time testing with portal** - Faster iteration than manual browser testing
5. **Event-driven design enables widgets** - Proper foundation for future expansion

## 🚨 DEVTOOLS INTEGRATION - SCREENSHOT & LOGGING FALLBACK SYSTEM

**📸 ROBUST SCREENSHOT ABSTRACTION** - Works even when Continuum is down!

### Current Implementation: Non-Production Demo Scripts
All scripts located in: `python-client/demos/devtools/`

#### Primary System: `start_devtools_system.py`
```bash
# Complete DevTools system - works independent of Continuum server
python python-client/demos/devtools/start_devtools_system.py

# Features:
# ✅ Opera GX auto-launch with --remote-debugging-port=9222
# ✅ Persistent DevTools daemon monitoring
# ✅ Real-time browser console log forwarding  
# ✅ DevTools Protocol screenshot capture (not html2canvas)
# ✅ Health monitoring with auto-recovery
# ✅ Works even if localhost:9000 is down
```

### Planned Production Architecture: Screenshot Fallback Chain

#### **Target Implementation:**
```javascript
// In src/commands/browser/screenshot/ScreenshotCommand.cjs
async function takeScreenshot(params) {
    // 1. Try DevTools Protocol (fastest, most reliable)
    if (await checkDevToolsAvailable()) {
        return await devtoolsCapture(params);
    }
    
    // 2. Fallback to html2canvas (current working method)
    if (await checkBrowserConnected()) {
        return await html2canvasCapture(params);
    }
    
    // 3. Emergency fallback via portal daemon (always works)
    return await portalDevToolsCapture(params);
}

async function checkDevToolsAvailable() {
    try {
        const response = await fetch('/api/devtools/status');
        return response.ok && (await response.json()).connected;
    } catch {
        return false;
    }
}

async function portalDevToolsCapture(params) {
    // Route to portal DevTools daemon system
    // This works even if Continuum server is completely down
    const response = await fetch('/api/portal/devtools/screenshot', {
        method: 'POST',
        body: JSON.stringify(params)
    });
    return await response.json();
}
```

### Testing & Verification
**Current Test Location:** `python-client/demos/devtools/testing/`
- `test_direct_devtools.py` - Raw DevTools Protocol testing
- `test_screenshot.py` - Daemon-based capture testing
- `quick_screenshot_test.py` - Portal integration testing

### Logging Abstraction Plan

#### **Multi-Source Log Aggregation:**
```bash
# Portal can always provide logs even when Continuum is down
python ai-portal.py --logs 5    # Gets logs from ALL sources:
                                # 1. Browser console (via DevTools)
                                # 2. Server logs (if available)
                                # 3. Daemon logs (always available)
                                # 4. Portal buffer logs (always available)
```

#### **Robust Log Sources (Priority Order):**
1. **DevTools browser console** - Real-time via WebSocket (fastest)
2. **Server logs** - Direct file access when server running
3. **Portal daemon logs** - Persistent storage (always available)
4. **Emergency buffer** - Local file fallback (never fails)

### Production Integration Roadmap

#### **Phase 1: Portal Integration** (Immediate)
```bash
# Modify ai-portal.py to use production DevTools system
python ai-portal.py --devtools  # Should trigger start_devtools_system.py
python ai-portal.py --screenshot # Should use DevTools if available
```

#### **Phase 2: Command Abstraction** (Next)
- Update `src/commands/browser/screenshot/` to detect DevTools
- Implement fallback chain: DevTools → html2canvas → portal daemon
- Add `/api/devtools/*` endpoints in continuum-core.cjs

#### **Phase 3: Universal Daemon Integration** (Later)  
- All daemons get DevTools capability
- Real-time log subscription with <100ms latency
- Complete abstraction - user never knows which method is used

### Robust System Design Philosophy

**🛡️ RESILIENCE PRINCIPLE**: System provides feedback no matter what's broken

1. **Independent Operation** - Portal can work without Continuum server
2. **Multiple Fallbacks** - Always have working screenshot and log methods
3. **Health Detection** - Automatically choose best available method
4. **Graceful Degradation** - Transparent fallback without user intervention
5. **Emergency Access** - Portal daemon provides last-resort capabilities

### Current Status: Ready for Integration
- ✅ **DevTools system fully working** in demo scripts
- ✅ **Real-time logging proven** with millisecond latency
- ✅ **Screenshot capture verified** via DevTools Protocol
- ✅ **Independent operation confirmed** - works without server
- ✅ **Health monitoring implemented** with auto-recovery
- 🎯 **Ready for portal integration** and command abstraction

## Tips for Success

1. **Start Small**: Pick one 🔴 command, investigate 5-10 minutes
2. **Document Everything**: Your notes help the next person
3. **Test Changes**: Always verify fixes work
4. **Use Logs**: Sentinel and debug output are your friends
5. **Follow the Breadcrumbs**: Check what the last agent learned
6. **✅ USE THE PROVEN PROCESS ABOVE** - Modular + Log-Driven + Incremental Testing

Remember: Even 5 minutes of investigation and documentation helps the whole team!