# DevTools Session Coordination Analysis

## Current Flow During Git Hook Verification

Based on codebase analysis, here's the exact sequence of browser launches during git hook verification:

### 1. Git Hook Trigger
```
git commit → .git/hooks/pre-commit → quick_commit_check.py
```

### 2. Browser Launch Sequence

#### Launch Point 1: `quick_commit_check.py` (Line 109-111)
```python
result = subprocess.run([
    sys.executable, 'devtools_full_demo.py', '--commit-check'
], capture_output=True, text=True, timeout=60)
```

#### Launch Point 2: `devtools_full_demo.py` (Line 235-239)
```python
# Inside launch_debug_opera() method
self.opera_process = subprocess.Popen(
    opera_cmd,  # Contains Opera with --remote-debugging-port=9222
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL
)
```

**Opera Command:**
```
/Applications/Opera GX.app/Contents/MacOS/Opera
--remote-debugging-port=9222
--disable-web-security
--user-data-dir=/tmp/opera-devtools-portal
http://localhost:9000
```

#### Launch Point 3: `devtools_full_demo.py` (Line 299-301)
```python
# Inside start_realtime_monitoring() method
self.monitor_process = subprocess.Popen([
    sys.executable, 'python-client/demos/devtools/realtime_devtools_demo.py'
], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1)
```

#### Launch Point 4: `take_devtools_screenshot.py` (Line 39)
```python
# If no existing DevTools connection found
daemon_id = await start_devtools_daemon("localhost:9000", [9222, 9223])
```

#### Launch Point 5: `devtools_daemon.py` (Line 183)
```python
# Inside _heal_devtools_port() healing process
subprocess.Popen(opera_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
```

**Healing Opera Command:**
```
/Applications/Opera GX.app/Contents/MacOS/Opera
--remote-debugging-port=9222  # or 9223
--disable-web-security
--user-data-dir=/tmp/opera-devtools-9222  # port-specific
http://localhost:9000
```

## Why 2 Browser Tabs Appear Initially

### Root Cause Analysis

1. **Primary Browser Launch**: `devtools_full_demo.py` launches Opera with user-data-dir `/tmp/opera-devtools-portal`

2. **Subprocess Monitoring**: `realtime_devtools_demo.py` is launched as a subprocess, which might attempt its own DevTools connection

3. **Screenshot Service**: `take_devtools_screenshot.py` attempts to use existing connection, but if it fails, launches `start_devtools_daemon()`

4. **Healing Process**: DevTools daemon has healing logic that can launch additional Opera instances on different ports (9222, 9223) with different user-data-dirs

5. **Race Condition**: Multiple components racing to establish DevTools connections, each potentially launching browsers

### Browser Instance Matrix

| Launch Point | User Data Dir | Port | URL | Purpose |
|--------------|---------------|------|-----|---------|
| devtools_full_demo.py | /tmp/opera-devtools-portal | 9222 | localhost:9000 | Primary verification |
| realtime_devtools_demo.py | Various | 9222/9223 | localhost:9000 | Monitoring subprocess |
| devtools_daemon healing | /tmp/opera-devtools-9222 | 9222 | localhost:9000 | Healing attempt |
| devtools_daemon healing | /tmp/opera-devtools-9223 | 9223 | localhost:9000 | Port fallback |

## Multi-Session Coordination Design

### Problem: No Session Manager
- Each component launches browsers independently
- No coordination between different DevTools sessions
- No session lifecycle management
- No artifact isolation between sessions

### Solution: Session Coordinator Architecture

```python
class DevToolsSessionCoordinator:
    """Central coordinator for all DevTools sessions"""
    
    def __init__(self):
        self.active_sessions = {}
        self.session_artifacts = {}
        self.coordination_lock = asyncio.Lock()
    
    async def request_session(self, session_id: str, purpose: str) -> DevToolsSession:
        """Request a coordinated DevTools session"""
        async with self.coordination_lock:
            if session_id in self.active_sessions:
                return self.active_sessions[session_id]
            
            session = await self._create_session(session_id, purpose)
            self.active_sessions[session_id] = session
            return session
    
    async def _create_session(self, session_id: str, purpose: str) -> DevToolsSession:
        """Create isolated session with dedicated artifacts"""
        session = DevToolsSession(
            session_id=session_id,
            purpose=purpose,
            user_data_dir=f"/tmp/opera-devtools-{session_id}",
            port=self._allocate_port(),
            artifact_dir=f".continuum/sessions/{session_id}"
        )
        
        await session.initialize()
        return session

class DevToolsSession:
    """Individual DevTools session with isolated artifacts"""
    
    def __init__(self, session_id: str, purpose: str, user_data_dir: str, 
                 port: int, artifact_dir: str):
        self.session_id = session_id
        self.purpose = purpose
        self.user_data_dir = user_data_dir
        self.port = port
        self.artifact_dir = Path(artifact_dir)
        self.browser_process = None
        self.screenshots = []
        self.logs = []
    
    async def initialize(self):
        """Initialize session with dedicated browser and artifact storage"""
        # Create artifact directory
        self.artifact_dir.mkdir(parents=True, exist_ok=True)
        
        # Launch dedicated browser instance
        await self._launch_browser()
        
        # Setup artifact collection
        await self._setup_artifact_collection()
    
    async def capture_screenshot(self, filename: str) -> str:
        """Capture screenshot to session artifact directory"""
        screenshot_path = self.artifact_dir / "screenshots" / f"{filename}.png"
        screenshot_path.parent.mkdir(exist_ok=True)
        
        # Use session's dedicated DevTools connection
        # ... screenshot logic ...
        
        self.screenshots.append(screenshot_path)
        return str(screenshot_path)
    
    async def cleanup(self):
        """Clean up session resources"""
        if self.browser_process:
            self.browser_process.terminate()
        
        # Archive artifacts if needed
        await self._archive_artifacts()
```

### Session-Based Git Hook Flow

```python
# In quick_commit_check.py
async def run_verification():
    coordinator = DevToolsSessionCoordinator()
    
    # Request dedicated session for git verification
    session = await coordinator.request_session(
        session_id=f"git-verification-{commit_sha[:8]}",
        purpose="git_commit_verification"
    )
    
    try:
        # All verification uses this session
        screenshot = await session.capture_screenshot("verification")
        logs = await session.collect_logs()
        
        # Session artifacts are isolated
        return VerificationResult(
            session_id=session.session_id,
            screenshots=[screenshot],
            logs=logs,
            artifact_dir=session.artifact_dir
        )
    finally:
        await session.cleanup()
```

## Recommendations

### 1. Immediate Fix (Coordination)
- Add session coordinator to prevent duplicate browser launches
- Implement browser instance sharing between components
- Add timeout and cleanup for failed connections

### 2. Medium-term (Session Management)
- Implement full session-based architecture
- Isolate artifacts per session
- Add session lifecycle management

### 3. Long-term (Multi-Agent Support)
- Each AI agent gets dedicated session
- Session handoff between agents
- Persistent session storage for long-running tasks

### Implementation Priority

1. **Phase 1**: Add coordination checks to prevent duplicate launches
2. **Phase 2**: Implement basic session management
3. **Phase 3**: Full multi-session architecture with artifact isolation
4. **Phase 4**: Multi-agent session coordination

This design ensures each session is its own sandbox with dedicated artifacts while preventing the current browser multiplication issue.