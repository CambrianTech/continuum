# Process Isolation Architecture

## Overview

Continuum's execution architecture separates concerns through OS-level process isolation, enabling true sandboxing, resource control, and security boundaries while maintaining session continuity.

## Environment-Based Process Architecture

### Core Principle
Each `ContinuumEnvironment` type runs in its own OS process, with system-wide daemons coordinating between them.

### Process Types

#### System-Wide Daemons (Global OS Processes)
```bash
continuum-websocket-daemon     # PID 1001 - WebSocket routing & message dispatch
continuum-session-daemon       # PID 1002 - Session management & state tracking
continuum-browser-daemon       # PID 1003 - Browser operations & automation
continuum-renderer-daemon      # PID 1004 - HTML rendering & UI generation
```

#### Environment-Specific Executors (Isolated OS Processes)
```bash
continuum-server-executor      # PID 2001 - server.log operations (local)
continuum-remote-executor      # PID 2002 - remote.log operations (external)
continuum-agent-executor       # PID 2003 - agent.log operations (AI agents)
continuum-persona-executor     # PID 2004 - persona.log operations (internal AI)
```

## Environment Types & Log Routing

### ContinuumEnvironment Mapping
```typescript
type ContinuumEnvironment = 'browser' | 'server' | 'remote' | 'agent' | 'persona';
```

**Environment definitions:**
- `'browser'`: Human user interactions through web interface → `browser.log`
- `'server'`: Local server-side operations and daemons → `server.log`  
- `'remote'`: Commands from other machines (git hooks, remote clients) → `remote.log`
- `'agent'`: External AI users (agent-based users like Claude) → `agent.log`
- `'persona'`: Internal AI personas/identities within Continuum → `persona.log`

## Session Continuity Across Processes

### Session Routing Logic
1. **Initial Connect**: Creates session in appropriate executor process based on environment
2. **Session Join**: Routes to SAME executor process using existing sessionId
3. **Context Preservation**: Same logs, same state, same resources throughout session

### Example Flow
```typescript
// Initial connect creates session in remote process
const remoteContext = continuumContextFactory.create({
  sessionId: "abc123",
  environment: 'remote'
});
// → Routes to continuum-remote-executor (PID 2002)

// Later session_join uses SAME sessionId, SAME process
const joinContext = continuumContextFactory.create({
  sessionId: "abc123",  // Same session!
  environment: 'remote' // Same process!
});
// → Routes to continuum-remote-executor (PID 2002)
```

## Sandboxing Benefits

### Security Sandboxing
- **Process Isolation**: Each environment in separate memory space
- **Permission Boundaries**: Different OS-level permissions per process
- **Resource Limits**: cgroups/systemd limits per environment type
- **Network Isolation**: Separate network namespaces per environment

### Example Security Configurations
```bash
# Remote executor - restricted permissions
continuum-remote-executor   # chroot to /var/continuum/remote-sandbox/
                           # No local file access
                           # Limited network to approved hosts

# Agent executor - API-only access  
continuum-agent-executor   # chroot to /var/continuum/agent-sandbox/
                          # Network access only to AI APIs
                          # CPU/memory limits via systemd

# Server executor - full local access
continuum-server-executor  # Full local permissions
                          # Access to all local resources
```

### Operational Benefits
- **Crash Isolation**: One process failure doesn't affect others
- **Resource Control**: Independent CPU/memory limits
- **Monitoring**: Standard OS tools (`ps`, `top`, `htop`) show all processes
- **Debugging**: `tail -f remote.log` shows all external activity instantly
- **Scaling**: Each process can run on different cores/machines

## Implementation Strategy

### Phase 1: Current State
- Single process with environment-based logging
- Session continuity within single process
- UniversalLogger routing by environment type

### Phase 2: Process Separation
- Split executors into separate OS processes
- Maintain session routing via IPC/sockets
- Preserve existing session management logic

### Phase 3: Full Sandboxing
- Implement security boundaries per process
- Add resource limits via systemd/cgroups
- Network isolation and access control

## Architecture Principles

1. **Environment Determines Process**: Each ContinuumEnvironment type gets its own executor process
2. **Session Determines Routing**: Within a process, sessions route to same context
3. **Daemons Coordinate**: System-wide daemons manage inter-process communication
4. **Security by Design**: Process boundaries provide natural security isolation
5. **Observability**: Each environment type has dedicated log streams

This architecture provides true isolation while maintaining the session continuity and logging patterns already established in the current system.