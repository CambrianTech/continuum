# ArtifactsDaemon - Filesystem Access Orchestration

## **🎯 Mission**
Centralized filesystem access layer that enforces `.continuum` directory structure and provides session-isolated file operations for secure multi-agent environments.

## **🏗️ Architecture**

### **Session Isolation Design**
```
.continuum/jtag/
├── system/                          # System-wide shared resources
│   ├── config/                      # Global daemon configurations  
│   ├── personas/base-models/        # Shared base models & checkpoints
│   ├── logs/                        # System-wide operational logs
│   └── data/                        # Cross-session shared data
└── sessions/user/                   # Session-isolated workspaces
    ├── {persona-session-id}/        # LoRA-adapted persona workspace
    │   ├── checkpoints/             # Persona-specific model weights
    │   ├── training-data/           # Isolated training datasets
    │   ├── screenshots/             # Session screenshots & artifacts
    │   └── data/                    # Session-specific data storage
    ├── {human-session-id}/          # Human user session
    │   ├── screenshots/             # User interaction captures
    │   ├── logs/                    # Session-specific logs
    │   └── workspace/               # User temporary files
    └── {ci-session-id}/             # CI/CD isolated workspace
        ├── artifacts/               # Build artifacts & test outputs
        ├── logs/                    # CI-specific execution logs
        └── reports/                 # Test reports & coverage data
```

### **Operations & Path Resolution**
```typescript
// Session-specific operations (most common)
await artifactsDaemon.write({
  operation: 'write',
  relativePath: 'screenshots/debug-view.png',    // → sessions/user/{sessionId}/screenshots/
  content: imageBuffer,
  sessionId: userSessionId  // From command payload
});

// System-wide operations (shared resources)
await artifactsDaemon.write({
  operation: 'write', 
  relativePath: 'personas/base-models/foundation.json',  // → system/personas/base-models/
  content: modelData,
  sessionId: 'system'  // Special system session
});
```

## **🔐 Security & Isolation Benefits**

### **Multi-Agent Safety**
- **Persona Isolation**: Each LoRA-adapted persona operates in isolated session directory
- **Human Session Privacy**: User interactions and files contained within session boundary
- **CI/CD Sandboxing**: Build processes can't interfere with user sessions or other builds
- **Agent Isolation**: Claude instances get isolated workspace for temporary operations

### **Permission Architecture Compatibility**
- **Directory-level access control**: Sessions can only access their own directory + authorized system directories
- **Command-level permissions**: ArtifactsDaemon enforces which operations are allowed per session type
- **Audit trail**: All filesystem operations logged with session attribution
- **Resource quotas**: Future implementation can limit storage per session

## **🚀 Implementation Pattern**

### **Symmetric Daemon Architecture**
```
daemons/artifacts-daemon/
├── shared/ArtifactsDaemon.ts        # Universal filesystem interface (85% of logic)
├── browser/ArtifactsDaemonBrowser.ts # Thin delegation layer (5% browser-specific)  
├── server/ArtifactsDaemonServer.ts   # Actual filesystem operations (10% server-specific)
└── README.md                        # This file
```

### **Operations Supported**
- **`read`**: Read file content with session path resolution
- **`write`**: Atomic file writes with directory creation
- **`append`**: Append to files with session isolation  
- **`mkdir`**: Create directories within session structure
- **`list`**: List directory contents with proper filtering
- **`stat`**: Get file/directory metadata and permissions
- **`delete`**: Remove files/directories with safety checks

### **Integration Points**
```typescript
// Commands integrate via payload routing
export class FileSaveServerCommand {
  async execute(params: FileSaveParams): Promise<FileSaveResult> {
    // Delegate to ArtifactsDaemon instead of direct fs calls
    const result = await this.router.routeToServer('artifacts', {
      operation: 'write',
      relativePath: params.filepath,
      content: params.content,
      sessionId: params.sessionId,  // Session context from command
      options: { 
        createDirectories: true,
        atomicWrite: true
      }
    });
    
    return createFileSaveResult(params.context, params.sessionId, result);
  }
}
```

## **🎨 Design Philosophy**

### **Centralized Control, Distributed Access**
- **Single Authority**: ArtifactsDaemon is the only component with direct filesystem access
- **Universal Interface**: Same API for browser delegation and server execution
- **Session Awareness**: Every operation carries session context for proper isolation
- **Path Enforcement**: No direct path manipulation - daemon validates and resolves all paths

### **Future-Proof Permission System**
The session-based isolation creates natural boundaries for implementing:
- **Role-based access control**: Different session types get different permissions
- **Resource quotas**: Storage limits per session type
- **Audit logging**: Complete filesystem operation history with attribution
- **Cross-session sharing**: Controlled access to shared system resources

## **🔧 Usage Examples**

### **Screenshot (Session-Specific)**
```typescript
// Browser command creates screenshot in user session
const result = await jtag.commands.screenshot({
  querySelector: 'body',
  sessionId: userSessionId
});
// File saved to: .continuum/jtag/sessions/user/{userSessionId}/screenshots/
```

### **Persona Checkpoint (System-Wide)**
```typescript
// Persona saves training checkpoint to shared system directory
const result = await artifactsDaemon.write({
  operation: 'write',
  relativePath: 'personas/checkpoints/claude-v2-coding.bin',
  content: modelWeights,
  sessionId: 'system'  // System-wide storage
});
// File saved to: .continuum/jtag/system/personas/checkpoints/
```

### **CI Build Artifacts (Session-Isolated)**
```typescript
// CI process saves test results in isolated session
const result = await artifactsDaemon.write({
  operation: 'write', 
  relativePath: 'artifacts/test-results.json',
  content: testData,
  sessionId: ciBuildSessionId
});
// File saved to: .continuum/jtag/sessions/user/{ciBuildSessionId}/artifacts/
```

## **🛡️ Security Model**

### **Session Boundaries**
- **Strict Isolation**: Sessions cannot access each other's directories
- **Explicit Sharing**: System directory access requires explicit system sessionId
- **Path Validation**: All paths validated and normalized within .continuum structure
- **No Escaping**: Path traversal attacks prevented by controlled resolution

### **Future Permission Integration**
The ArtifactsDaemon provides natural integration points for:
- **ACL Systems**: Map sessionId to permissions and allowed operations
- **Quota Management**: Track storage usage per session with limits
- **Audit Systems**: Log all filesystem operations with full context
- **Encryption**: Encrypt sensitive session data at rest

This creates the foundation for **truly autonomous multi-agent systems** where each agent (persona, human, CI) operates safely in isolation while sharing system resources through controlled interfaces.