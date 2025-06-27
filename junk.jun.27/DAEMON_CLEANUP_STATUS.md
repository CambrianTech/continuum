# DAEMON CLEANUP STATUS - READ THIS FIRST

## CRITICAL UNDERSTANDING
The WebSocketDaemon (1193 lines) is doing WAY TOO MUCH. It's duplicating functionality that other daemons were specifically designed to handle.

## CORRECT ARCHITECTURE
- **WebSocketDaemon**: Just WebSocket routing. Nothing else.
- **RendererDaemon**: HTML, CSS, static files, status pages, API endpoints
- **CommandProcessorDaemon**: Command execution via WebSocket

## WHAT TO DELETE FROM WebSocketDaemon
These methods are DEAD CODE that duplicate RendererDaemon functionality:

```typescript
// DELETE THESE - they belong in RendererDaemon:
private async generateStatusPage(): Promise<string> // Line 543
private async getAgentsData(): Promise<any> // Line 475  
private async getPersonasData(): Promise<any> // Line 497
private async proxyToRendererDaemon() // Line 361 - NEVER CALLED
private async requestFromDaemon() // Line 535 - NEVER CALLED
```

## CURRENT MODULARIZATION PROGRESS
✅ Created `DaemonRouter.ts` (18 lines) - Simple routing: WebSocket→Commands, HTTP→Renderer
✅ Created `StatusPageGenerator.ts` - Moved from WebSocketDaemon 
✅ Created `HttpRequestHandler.ts` - For proper HTTP handling
✅ Added unit tests for new modules
✅ Fixed memory leaks in BaseDaemon (heartbeat interval cleanup)

## FILES OVER 400 LINES TO BREAK DOWN
```
1193 src/integrations/websocket/WebSocketDaemon.ts ← PRIORITY 1
 912 src/academy/lora-package-manager.ts
 820 src/mesh/SemanticDependencyResolver.ts  
 813 src/commands/core/help/HelpCommand.ts
 701 src/daemons/mesh-coordinator/MeshCoordinatorDaemon.ts
 694 src/daemons/renderer/RendererDaemon.ts
 577 src/daemons/command-processor/CommandProcessorDaemon.ts
```

## DEAD CODE FOUND
- `proxyToRendererDaemon()` - Method exists but NEVER CALLED
- `requestFromDaemon()` - Generic method NEVER USED  
- `getAgentsData()` - Returns hardcoded dummy data
- `getPersonasData()` - Returns hardcoded dummy data
- HTML generation in WebSocketDaemon - RendererDaemon should do this

## TYPE ASSERTION HACKS (468 found)
Examples: `(widget as any).loadCSS()` - Testing private methods by casting to any
Fix: Test public behavior, not private methods

## MEMORY LEAKS FIXED
✅ BaseDaemon heartbeat interval now clears on stop
✅ CommandProcessorDaemon monitoring interval now clears on stop
Need to check: BrowserManagerDaemon, other setInterval usage

## NEXT STEPS
1. Delete dead methods from WebSocketDaemon
2. Integrate DaemonRouter into simplified WebSocketDaemon  
3. Break down other 400+ line files
4. Replace `as any` hacks with proper typing
5. Remove all unused imports and methods

## TEST STATUS
- BaseDaemon tests: ✅ PASSING (8/8)
- Other daemon tests: ❌ Import issues, need TS config fixes

The goal: WebSocketDaemon should be ~200 lines max, just routing.