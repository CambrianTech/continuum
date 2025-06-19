# Continuum DevTools Integration Plan

## 🎯 Overview

Complete integration of DevTools Protocol into Continuum for seamless browser automation, real-time logging, and screenshot capture.

## 🏗️ Architecture

### Current State: ✅ WORKING
- Opera launches in debug mode with `--remote-debugging-port=9222`
- DevTools daemon connects and monitors browser console logs
- Screenshot capture via DevTools Protocol
- Persistent monitoring with real-time log streaming

### Target State: 🎯 FULL INTEGRATION
- `--devtools` automatically launches entire system
- Screenshot command intelligently chooses DevTools vs html2canvas
- All daemon processes have DevTools integration
- Real-time log subscription from anywhere
- Seamless API abstraction

## 📋 Implementation Plan

### Phase 1: Portal Integration ✅ COMPLETE
```bash
# These commands should trigger full DevTools system:
python ai-portal.py --devtools      # Primary command
python ai-portal.py --failsafe      # Emergency recovery
python ai-portal.py --connect       # Auto-detection mode
```

**Implementation:**
- Modify `start_devtools_daemon()` in ai-portal.py to use new DevToolsDaemon
- Add auto-browser launch logic
- Integrate with existing `--connect` flow

### Phase 2: Screenshot Command Adaptation
**Location:** `/Users/joel/Development/cambrian/continuum/src/commands/browser/screenshot`

**Current Logic:**
```javascript
// ScreenshotCommand.cjs - CURRENT
async function takeScreenshot(params) {
    // Always uses html2canvas
    return await html2canvasCapture(params);
}
```

**Target Logic:**
```javascript
// ScreenshotCommand.cjs - TARGET
async function takeScreenshot(params) {
    // Check if DevTools is available
    const devtoolsAvailable = await checkDevToolsStatus();
    
    if (devtoolsAvailable) {
        console.log('📸 Using DevTools Protocol for screenshot');
        return await devtoolsCapture(params);
    } else {
        console.log('📸 Using html2canvas fallback');
        return await html2canvasCapture(params);
    }
}

async function checkDevToolsStatus() {
    // Check if DevTools daemon is running and connected
    try {
        const response = await fetch('/api/devtools/status');
        return response.ok && response.json().connected;
    } catch {
        return false;
    }
}

async function devtoolsCapture(params) {
    // Route to DevTools daemon for capture
    const response = await fetch('/api/devtools/screenshot', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(params)
    });
    return await response.json();
}
```

### Phase 3: DevTools Command Creation
**Location:** `/Users/joel/Development/cambrian/continuum/src/commands/devtools/`

**New Command Structure:**
```
src/commands/devtools/
├── DevToolsCommand.cjs          # Main command handler
├── DevToolsLauncher.cjs         # Browser launch logic  
├── DevToolsStatus.cjs           # Status checking
└── DevToolsProxy.cjs            # Python daemon proxy
```

**DevToolsCommand.cjs:**
```javascript
class DevToolsCommand {
    static command = "DEVTOOLS";
    static description = "Launch and manage DevTools browser debugging";
    
    static async execute(params, context) {
        const action = params.action || 'launch';
        
        switch (action) {
            case 'launch':
                return await this.launchDevTools(params);
            case 'status':
                return await this.getStatus();
            case 'screenshot':
                return await this.takeScreenshot(params);
            case 'stop':
                return await this.stopDevTools();
            default:
                throw new Error(`Unknown DevTools action: ${action}`);
        }
    }
    
    static async launchDevTools(params) {
        // Call Python daemon system
        const result = await exec('python python-client/start_devtools_system.py --daemon');
        return {
            success: true,
            message: 'DevTools system launched',
            data: result
        };
    }
}
```

### Phase 4: Universal Daemon Integration

**All Python daemons get DevTools capability:**
```python
# Base daemon class enhancement
class BaseDaemon:
    def __init__(self, daemon_type):
        self.daemon_type = daemon_type
        self.devtools_integration = DevToolsIntegration() if DEVTOOLS_AVAILABLE else None
        
    async def get_logs(self, source='all'):
        logs = []
        
        # Get daemon-specific logs
        logs.extend(self.get_daemon_logs())
        
        # Get DevTools browser logs if available
        if self.devtools_integration and source in ['all', 'browser']:
            logs.extend(await self.devtools_integration.get_browser_logs())
            
        # Get server logs if available  
        if source in ['all', 'server']:
            logs.extend(self.get_server_logs())
            
        return sorted(logs, key=lambda x: x['timestamp'])
```

### Phase 5: API Abstraction Layer

**Server-side API endpoints:**
```javascript
// continuum-core.cjs additions
app.get('/api/devtools/status', async (req, res) => {
    const status = await getDevToolsStatus();
    res.json(status);
});

app.post('/api/devtools/screenshot', async (req, res) => {
    const result = await routeToDevToolsDaemon('screenshot', req.body);
    res.json(result);
});

app.get('/api/devtools/logs', async (req, res) => {
    const logs = await getDevToolsLogs(req.query);
    res.json(logs);
});

app.post('/api/devtools/launch', async (req, res) => {
    const result = await launchDevToolsSystem(req.body);
    res.json(result);
});
```

## 🚀 Usage Examples

### End-to-End Script Usage
```bash
# Launch complete DevTools system
python start_devtools_system.py

# Output:
# 🎯 CONTINUUM DEVTOOLS SYSTEM STARTUP
# 🔧 Cleaning up existing Opera processes...
# 🚀 Launching Opera GX in debug mode...
# 🔌 Starting persistent DevTools monitoring...
# 📸 Taking test screenshot: devtools_system_test
# 🎉 DEVTOOLS SYSTEM FULLY OPERATIONAL
```

### Portal Integration
```bash
# Any of these should trigger full system:
python ai-portal.py --devtools
python ai-portal.py --failsafe  
python ai-portal.py --connect --devtools-mode

# Status checking:
python ai-portal.py --daemons
python ai-portal.py --logs 5
```

### Continuum Command Usage
```javascript
// In Continuum chat or API:
/devtools launch
/devtools status  
/devtools screenshot filename=test.png
/screenshot        // Automatically uses DevTools if available
```

### Python API Usage
```python
# Direct daemon usage
from continuum_client.devtools.devtools_daemon import start_devtools_daemon

daemon_id = await start_devtools_daemon()
screenshot_path = await daemon.capture_screenshot("my_screenshot")

# Via daemon manager
from continuum_client.core.daemon_manager import daemon_manager

# Get all DevTools logs
logs = await daemon_manager.get_devtools_logs(lines=50)

# Take screenshot via any daemon
result = await daemon_manager.handle_screenshot_request(daemon_id, {
    'filename': 'automated_capture.png',
    'format': 'png',
    'quality': 90
})
```

## 🔧 Implementation Tasks

### Immediate (This Session)
- [x] Create end-to-end automation script
- [x] Document integration architecture
- [ ] Test script with current system
- [ ] Validate screenshot + logging workflow

### Near-term (Next Session)
- [ ] Modify ai-portal.py `--devtools` to use new system
- [ ] Create DevTools command in Continuum commands
- [ ] Add DevTools status API endpoints
- [ ] Implement screenshot command adaptation

### Medium-term
- [ ] Universal daemon DevTools integration
- [ ] Real-time log subscription system
- [ ] Web UI for DevTools management
- [ ] Auto-detection and fallback logic

### Long-term
- [ ] Multi-browser support (Chrome, Firefox)
- [ ] Remote DevTools capabilities
- [ ] Performance monitoring integration
- [ ] Automated testing integration

## 🎯 Success Criteria

### ✅ Phase 1 Complete When:
- `python ai-portal.py --devtools` launches entire system automatically
- Opera opens to localhost:9000 in debug mode
- DevTools daemon starts and stays persistent
- Real-time logs stream continuously
- Screenshots work on-demand

### ✅ Phase 2 Complete When:
- `/screenshot` command intelligently chooses DevTools vs html2canvas
- Both methods produce identical results
- Seamless fallback when DevTools unavailable
- No user-visible difference in API

### ✅ Full Integration Complete When:
- All commands work identically regardless of method
- Real-time logs available system-wide
- Zero manual setup required
- Production-ready reliability

## 📁 File Structure

```
continuum/
├── start_devtools_system.py           # End-to-end automation
├── python-client/
│   ├── ai-portal.py                   # Modified --devtools integration
│   ├── continuum_client/
│   │   ├── devtools/
│   │   │   ├── devtools_daemon.py     # ✅ Core daemon (working)
│   │   │   └── log_monitor.py         # ✅ Log monitoring (working)
│   │   └── core/
│   │       └── daemon_manager.py      # ✅ Daemon management (working)
└── src/
    ├── commands/
    │   ├── browser/
    │   │   └── screenshot/            # 🎯 Needs DevTools integration
    │   └── devtools/                  # 🆕 New DevTools commands
    └── core/
        └── continuum-core.cjs         # 🎯 Needs API endpoints
```

This integration plan provides a complete path from the current working system to full production integration with seamless API abstraction.