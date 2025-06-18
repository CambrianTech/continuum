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

### Essential Commands
```bash
# Status and monitoring
python python-client/ai-portal.py --dashboard    # Full AI agent dashboard
python python-client/ai-portal.py --roadmap     # Complete restoration strategy
python python-client/ai-portal.py --broken      # See broken commands
python python-client/ai-portal.py --cmd tests   # Run unit tests

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

## 🚨 CRITICAL REQUIREMENT - LOG MONITORING

**MUST SEE LOGS FROM CLIENT BROWSER AND SERVER TO PROCEED WITH DEVELOPMENT. IF BROKEN DIAGNOSE AND FIX. NO OTHER CODING CAN HAPPEN TILL THIS IS RESOLVED.**

### Log Monitoring Status Check
```bash
python python-client/ai-portal.py --logs 3    # MUST show client and server activity
python python-client/ai-portal.py --connect   # MUST establish connection monitoring
```

If logs are broken or show "No WebSocket connections":
1. **STOP ALL OTHER WORK**
2. **FIX LOG MONITORING FIRST** 
3. **DIAGNOSE SERVER/CLIENT CONNECTION**
4. **NO CODING UNTIL LOGS WORK**

## Key Reminder
- This is a **sophisticated AI platform**, not a simple tool
- All advanced capabilities are **git recoverable** with exact commands in `ROADMAP.md`
- **USE THE PORTAL SYSTEM** - don't reinvent with manual log monitoring
- **LOG MONITORING IS PREREQUISITE FOR ALL DEVELOPMENT**
- The automation foundation works - integration gaps need fixing
