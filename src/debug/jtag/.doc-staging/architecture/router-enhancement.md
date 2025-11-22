# Router Enhancement Proposal - Standardized Daemon Coordination

## **🚨 Current Problems**

### **1. Missing Helper Methods**
Documentation references non-existent methods:
```typescript
// DOCUMENTED BUT NOT IMPLEMENTED
await router.routeToServer('artifacts', operation);
await router.routeToBrowser('command', operation);  
```

### **2. Three Inconsistent Communication Patterns**

**Pattern A: Manual Message Construction** (DataDaemon, ArtifactsDaemon)
```typescript
const message = JTAGMessageFactory.createRequest(context, 'browser', 'server/data', payload, correlationId);
const response = await this.router.postMessage(message);
```

**Pattern B: Command RemoteExecute** (Screenshot, FileSave)
```typescript
const result = await this.remoteExecute(params, 'file/save', this.context.environment);
```

**Pattern C: Direct Daemon Access** (SessionCreate)
```typescript
const sessionDaemon = this.commander.router.getSubscriber('session-daemon');
const response = await sessionDaemon.handleMessage(sessionMessage);
```

### **3. Complex Router Implementation**
Current `JTAGRouter` has 868 lines with:
- Transport strategy abstraction
- Message queuing with priority
- Health monitoring
- Response correlation
- P2P networking
- External client detection

**The router is complex but lacks simple coordination helpers.**

## **🎯 Enhancement Proposal**

### **1. Add Standard Daemon Coordination Methods**

```typescript
// Add to JTAGRouter interface
export interface RouterDaemonCoordination {
  // Standard daemon coordination
  routeToServer<T>(daemon: string, payload: any, sessionId?: UUID): Promise<T>;
  routeToBrowser<T>(daemon: string, payload: any, sessionId?: UUID): Promise<T>;
  routeToDaemon<T>(daemonName: string, payload: any, sessionId?: UUID): Promise<T>;
  
  // Daemon discovery
  isDaemonAvailable(daemonName: string): boolean;
  getDaemon<T extends MessageSubscriber>(daemonName: string): T | null;
  listAvailableDaemons(): string[];
}
```

### **2. Implementation in JTAGRouter**

```typescript
// Add to JTAGRouter class
export abstract class JTAGRouter extends JTAGModule implements RouterDaemonCoordination {
  
  /**
   * Route to server daemon with automatic message construction
   */
  async routeToServer<T>(daemon: string, payload: any, sessionId?: UUID): Promise<T> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'browser',
      `server/${daemon}`,
      { ...payload, sessionId: sessionId ?? this.context.uuid },
      JTAGMessageFactory.generateCorrelationId()
    );
    
    const result = await this.postMessage(message);
    if ('response' in result && result.response) {
      return result.response as T;
    }
    throw new Error(`No response received for ${daemon} operation`);
  }

  /**
   * Route to browser daemon with automatic message construction
   */
  async routeToBrowser<T>(daemon: string, payload: any, sessionId?: UUID): Promise<T> {
    const message = JTAGMessageFactory.createRequest(
      this.context,
      'server',
      `browser/${daemon}`,
      { ...payload, sessionId: sessionId ?? this.context.uuid },
      JTAGMessageFactory.generateCorrelationId()
    );
    
    const result = await this.postMessage(message);
    if ('response' in result && result.response) {
      return result.response as T;
    }
    throw new Error(`No response received for ${daemon} operation`);
  }

  /**
   * Route to daemon in current environment
   */
  async routeToDaemon<T>(daemonName: string, payload: any, sessionId?: UUID): Promise<T> {
    const daemon = this.getSubscriber(`${daemonName}-daemon`);
    if (!daemon) {
      throw new Error(`Daemon '${daemonName}' not available`);
    }

    const message = JTAGMessageFactory.createRequest(
      this.context,
      `${this.context.environment}/${daemonName}-daemon`,
      `${daemonName}-daemon`,
      { ...payload, sessionId: sessionId ?? this.context.uuid },
      JTAGMessageFactory.generateCorrelationId()
    );
    
    const result = await daemon.handleMessage(message);
    return result as T;
  }

  /**
   * Check if daemon is available
   */
  isDaemonAvailable(daemonName: string): boolean {
    return this.getSubscriber(`${daemonName}-daemon`) !== null;
  }

  /**
   * Get daemon instance with type safety
   */
  getDaemon<T extends MessageSubscriber>(daemonName: string): T | null {
    return this.getSubscriber(`${daemonName}-daemon`) as T | null;
  }

  /**
   * List all available daemons
   */
  listAvailableDaemons(): string[] {
    return this.endpointMatcher.getEndpoints()
      .filter(endpoint => endpoint.endsWith('-daemon'))
      .map(endpoint => endpoint.replace('-daemon', ''));
  }
}
```

### **3. Standardized Daemon Integration Pattern**

**Before (Manual Message Construction):**
```typescript
// DataDaemonBrowser - complex manual construction
const message = JTAGMessageFactory.createRequest(
  this.context,
  'browser', 
  'server/data',
  payload,
  `data_create_${Date.now()}`
);
const response = await this.router.postMessage(message);
```

**After (Simple Coordination Helper):**
```typescript
// DataDaemonBrowser - clean coordination
const response = await this.router.routeToServer('data', payload);
```

### **4. Command-to-Daemon Integration**

**Before (RemoteExecute Complexity):**
```typescript
// ScreenshotServerCommand - complex delegation
const saveResult: FileSaveResult = await this.remoteExecute(saveParams, 'file/save', this.context.environment);
```

**After (Direct Daemon Coordination):**
```typescript
// ScreenshotServerCommand - direct daemon coordination
const saveResult = await this.commander.router.routeToServer('artifacts', {
  operation: 'write',
  relativePath: `screenshots/${params.filename}`,
  content: imageBuffer,
  sessionId: params.sessionId
});
```

### **5. Enhanced FileSave Integration**

**Current (Direct FS Access):**
```typescript
// FileSaveServerCommand - bypasses daemon architecture
await fs.mkdir(path.dirname(fullPath), { recursive: true });
await fs.writeFile(fullPath, saveParams.content);
```

**Proposed (ArtifactsDaemon Integration):**
```typescript
// FileSaveServerCommand - proper daemon coordination
const artifactsResult = await this.commander.router.routeToDaemon('artifacts', {
  operation: 'write',
  relativePath: saveParams.filepath,
  content: saveParams.content,
  sessionId: saveParams.sessionId,
  options: { createDirectories: true, atomicWrite: true }
});

return createFileSaveResult(saveParams.context, saveParams.sessionId, {
  success: artifactsResult.success,
  filepath: artifactsResult.fullPath,
  bytesWritten: Buffer.byteLength(saveParams.content),
  created: true
});
```

## **🚀 Benefits**

### **1. Simplified Daemon Integration**
- **85% reduction** in message construction boilerplate
- **Consistent API** across all daemon coordination
- **Type safety** with automatic sessionId handling

### **2. Clear Architecture Boundaries**
- **Commands** coordinate with daemons via router helpers
- **Daemons** coordinate with other daemons via router helpers  
- **No direct daemon-to-daemon coupling**

### **3. Enhanced Debugging**
```typescript
// Router diagnostic helpers
console.log('Available daemons:', router.listAvailableDaemons());
console.log('Data daemon available:', router.isDaemonAvailable('data'));
console.log('Artifacts daemon:', router.getDaemon('artifacts'));
```

### **4. Migration Path**
1. **Add router helpers** - non-breaking enhancement
2. **Update daemons** - one by one to use helpers
3. **Update commands** - migrate from remoteExecute to daemon coordination
4. **Remove legacy patterns** - after full migration

## **🔧 Implementation Strategy**

### **Phase 1: Router Enhancement (Non-Breaking)**
- Add coordination helpers to JTAGRouter
- Maintain existing postMessage() functionality
- Add comprehensive tests for new methods

### **Phase 2: Daemon Migration**  
- Update DataDaemon to use `routeToServer('artifacts')`
- Update ArtifactsDaemon coordination patterns
- Validate no regression in existing functionality

### **Phase 3: Command Migration**
- Update ScreenshotServerCommand to use daemon coordination
- Update FileSaveServerCommand to use ArtifactsDaemon
- Remove direct filesystem access from commands

### **Phase 4: Legacy Cleanup**
- Remove redundant message construction patterns
- Simplify remoteExecute() for command-to-command calls only
- Update documentation to reflect standardized patterns

## **🎯 Success Criteria**

### **Consistency**
- **All daemon coordination** uses same router helper methods
- **All commands** coordinate with daemons through standardized interface
- **Zero direct daemon-to-daemon coupling**

### **Simplicity** 
- **One-line daemon coordination**: `await router.routeToServer('artifacts', payload)`
- **Automatic sessionId handling** in all coordination calls
- **Type-safe coordination** with proper TypeScript support

### **Architecture Integrity**
- **Commands coordinate with daemons** (not other commands)
- **Daemons coordinate with other daemons** (when needed)
- **Router mediates all coordination** (no direct coupling)

This enhancement transforms the complex router into a **simple, powerful coordination engine** while maintaining all existing transport and correlation capabilities.