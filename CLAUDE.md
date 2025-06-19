# Claude Development Guidelines for Continuum

## 🚨 CRITICAL CONTEXT: This is a Sophisticated AI Platform

This is NOT just a simple screenshot tool. Continuum is a **revolutionary AI training platform** with:
- **Academy system** for adversarial AI training (TestingDroid vs ProtocolSheriff) 
- **LoRA adapter system** with 190,735x storage reduction
- **Mass Effect-style cyberpunk UI** with slideout panels
- **Multi-agent coordination** and browser automation
- **35 working commands** + complete automation foundation

**🏛️ MAJOR DISCOVERY**: Advanced capabilities were lost but are **git recoverable**. See `ROADMAP.md` for complete restoration strategy.

## Portal Usage - DON'T BE A STROKE PATIENT

### Log Monitoring
- **NEVER** use `tail -f continuum-output.log` or manual log monitoring
- **ALWAYS** use the portal's built-in logging:
  ```bash
  python python-client/ai-portal.py --logs 10  # Check recent activity
  python python-client/ai-portal.py --connect  # Start persistent monitoring
  ```

**Failsafe Logging Architecture:**
- **Primary**: Real-time browser console + server logs via WebSocket forwarding
- **Failsafe**: Daemon system provides backup when WebSocket connections break
- **Emergency**: Direct daemon log file access when all else fails

```bash
# When primary logging breaks, use daemon failsafe:
python python-client/ai-portal.py --daemons           # See what's still running
python python-client/ai-portal.py --daemon-logs ID    # Read logs from disk
python python-client/ai-portal.py --failsafe          # Emergency recovery mode
```

This ensures you can **always accomplish debugging** even when browser connections fail.

### Essential Commands
```bash
# Status and monitoring
python python-client/ai-portal.py --dashboard    # Full AI agent dashboard
python python-client/ai-portal.py --roadmap     # Complete restoration strategy
python python-client/ai-portal.py --broken      # See broken commands
python python-client/ai-portal.py --cmd tests   # Run unit tests

# DevTools daemon management
python python-client/ai-portal.py --devtools    # Start DevTools daemon
python python-client/ai-portal.py --daemons     # List running daemons
python python-client/ai-portal.py --daemon-logs ID  # Get daemon logs
python python-client/ai-portal.py --failsafe    # Emergency recovery

# Screenshot capture with intelligent path routing
python python-client/ai-portal.py --cmd screenshot --filename myshot.png  # Uses Continuum's configured paths
# Daemon automatically uses screenshot dir from Continuum API settings

# Working automation (CRITICAL DISCOVERY)
python python-client/trust_the_process.py       # 336 lines of working browser automation!

# Screenshot testing (server works, needs browser connection)
python python-client/ai-portal.py --connect
python python-client/ai-portal.py --cmd screenshot --filename test.png
python python-client/ai-portal.py --logs 5
```

## 🔥 CRITICAL ISSUE: Browser Connection Required

**The Problem**: Screenshot orchestration works server-side but client execution fails
- ✅ Server: Command processing, filename parameter passing, WSTransfer orchestration
- ❌ Client: No WebSocket connections to http://localhost:9000
- ✅ Fix Applied: Added command handler to UIGenerator.cjs (v0.2.2077)

**Solution**: Open browser to http://localhost:9000 to enable client-side execution

## 🎯 Current Session Achievements

1. **Fixed screenshot filename parameter** - Portal tokenizer now passes `--filename` correctly
2. **Added client command handler** - UIGenerator.cjs now routes screenshot commands to ScreenshotCommandClient
3. **Incremented version** - v0.2.2077 includes command routing fix
4. **WSTransfer orchestration** - Complete workflow: html2canvas → WSTransfer → FileSave

## 📋 NEXT AGENT PRIORITIES

1. **IMMEDIATE**: Open browser to http://localhost:9000 and test screenshot end-to-end
2. **HIGH**: Check `ROADMAP.md` for git recovery commands for Mass Effect UI
3. **HIGH**: Connect `trust_the_process.py` automation to UI buttons (DEPLOY/RETRAIN/SHARE)
4. **MEDIUM**: Restore slideout panels and multi-agent selection (all git recoverable)

## 🚨 ABSOLUTELY NO MANUAL INTERVENTIONS

**STOP ALL MANUAL INTERVENTIONS COMPLETELY:**
- ❌ NO manual file edits
- ❌ NO manual version changes  
- ❌ NO manual node commands
- ❌ NO manual package.json edits
- ✅ Use ONLY the portal system
- ✅ Let auto-healing work
- ✅ Let the system manage itself

**MANUAL INTERVENTION DETECTED = PROCESS FAILURE**

### Code Editing Rules:
- ✅ **ALLOWED**: Edit code to fix bugs, syntax errors, add missing handlers
- ❌ **NOT ALLOWED**: Edit code to circumvent the automated process
- **The distinction**: **Fix the system** vs **Work around the system**

## 🚨 SEPARATION OF LANGUAGES

**Keep scripting languages in separate files:**
- ❌ NO embedding JavaScript in Python files
- ❌ NO embedding Python in JavaScript files  
- ❌ NO embedding CSS in JavaScript files
- ❌ NO embedding HTML in script files
- ✅ **Create separate files** for each language
- ✅ **Load external files** when needed in multiple places
- ✅ **Follow the Personas pattern** - CSS lives in its own file, not embedded

**One language per file. Reuse through imports/includes.**

## ✅ MILESTONE ACHIEVED - LOG MONITORING RESTORED

**🎉 CONSOLE FORWARDING FIX SUCCESSFUL** - Browser console logs now visible in portal!

### Fixed Issue: Browser Console Forwarding 
- **Problem**: `setupConsoleForwarding()` called before WebSocket connection established
- **Solution**: Moved console forwarding setup to AFTER WebSocket connection (continuum-api.js:390)
- **Result**: Portal now shows both client and server logs in real-time

### Log Monitoring Status: ✅ WORKING
```bash
python python-client/ai-portal.py --logs 3    # Shows client and server activity
python python-client/ai-portal.py --connect   # Establishes connection monitoring
```

**Evidence of Working Console Forwarding:**
- Browser console.log messages appear in portal logs
- JavaScript execution errors visible with stack traces
- Real-time debugging capability fully restored

### 🚨 CRITICAL REQUIREMENT - LOG MONITORING

**MUST SEE LOGS FROM CLIENT BROWSER AND SERVER TO PROCEED WITH DEVELOPMENT. IF BROKEN DIAGNOSE AND FIX. NO OTHER CODING CAN HAPPEN TILL THIS IS RESOLVED.**

If logs break again:
1. **STOP ALL OTHER WORK**
2. **CHECK WEBSOCKET CONNECTION TIMING** 
3. **VERIFY setupConsoleForwarding() called after WebSocket ready**
4. **NO CODING UNTIL LOGS WORK**

## ✅ MILESTONE ACHIEVED - CONTINUON EMOTION SYSTEM

**🎭 MODULAR EMOTION ARCHITECTURE IMPLEMENTED** - Priority-based status + temporary emotion display!

### Development Process That Worked:
1. **Modular Design**: Created `ContinuonStatus.cjs` as central system-wide status manager
2. **Priority System**: RED (errors) > YELLOW (connecting) > GREEN (healthy) > EMOTION 
3. **Duration Support**: Temporary emotions with auto-revert to status color
4. **Multi-Surface Display**: Both browser favicon AND continuon ring overlay
5. **Real-Time Testing**: Used portal logs + browser_js for debugging
6. **Event-Driven Ready**: Architecture prepared for widget conversion

### Implementation Stack:
```bash
src/core/ContinuonStatus.cjs           # Central status/emotion manager
src/commands/core/emotion/EmotionCommand.cjs  # User-facing emotion API
src/ui/UIGenerator.cjs                 # UI integration (favicon + ring overlay)
src/core/continuum-core.cjs           # WebSocket event wiring
```

### Testing Methodology:
```bash
# 1. Test WebSocket connection status changes
python3 ai-portal.py --cmd browser_js --params '{"script": "..."}'

# 2. Test emotion commands (with duration)
python3 ai-portal.py --cmd emotion --params '{"emotion": "wink", "duration": 3000}'

# 3. Monitor both client and server logs
tail -1 .continuum/ai-portal/logs/buffer.log

# 4. Direct UI testing via browser JavaScript
python3 ai-portal.py --cmd browser_js --params '{"script": "base64_test_code"}'
```

### Key Success Factors:
- **Log monitoring enabled** - Could see browser console + server logs simultaneously
- **Modular architecture** - Clean separation of concerns (no god objects)
- **Incremental testing** - Test each layer independently (status → UI → events)
- **Real-time debugging** - Portal logs showed WebSocket message flow
- **Direct JavaScript testing** - Bypassed command issues to test UI directly

### Verified Working Features:
✅ **Browser favicon dynamic emoji** (🔴→🟡→🟢→😉)  
✅ **Ring emotion overlay** (emoji appears on green dot)  
✅ **Priority system** (status always overrides emotion)  
✅ **Duration timers** (auto-revert after specified milliseconds)  
✅ **Event broadcasting** (WebSocket messages to all clients)  
✅ **Modular design** (ready for widget conversion)

### Example Working Commands:
```bash
# Quick wink for 2 seconds
python3 ai-portal.py --cmd emotion --params '{"emotion": "wink", "duration": 2000}'

# Celebration for 5 seconds  
python3 ai-portal.py --cmd emotion --params '{"emotion": "celebration", "duration": 5000}'

# Permanent mood until manually changed
python3 ai-portal.py --cmd emotion --params '{"emotion": "smile"}'
```

## Key Reminder
- This is a **sophisticated AI platform**, not a simple tool
- All advanced capabilities are **git recoverable** with exact commands in `ROADMAP.md`
- **USE THE PORTAL SYSTEM** - don't reinvent with manual log monitoring
- **LOG MONITORING IS PREREQUISITE FOR ALL DEVELOPMENT**
- The automation foundation works - integration gaps need fixing
