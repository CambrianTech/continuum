# Screenshot Implementation Action Plan

<!-- ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking -->

## 🎯 **Current Status: Screenshot Command Architecture Complete, Execution Missing**

The screenshot system follows a **RemoteCommand** pattern designed for browser-server communication, but the critical WebSocket execution pipeline is incomplete.

## 🚨 **Root Cause Analysis**

### Error Message:
```
"Real WebSocket communication not yet implemented - needs browser message handler"
```

### Location:
`src/commands/core/remote-command/RemoteCommand.ts:147`

### Architecture Gap:
- ✅ **Server→Browser Commands**: Can send commands TO server
- ❌ **Browser←Server Commands**: Cannot receive commands FROM server  
- ❌ **html2canvas Integration**: No screenshot capture capability
- ❌ **Bidirectional Pipeline**: Incomplete execution flow

## 📊 **Implementation Phases**

### **Phase 1: Browser Message Handler** (Priority: HIGH, Est: 2-3 hours)

**Files to Modify:**
- `src/ui/continuum-browser-client/connection/WebSocketManager.ts`
- `src/ui/continuum-browser-client/types/WebSocketTypes.ts`

**Tasks:**
1. Add `remote_execution_request` message type handler
2. Create browser-side command executor 
3. Implement response correlation with `requestId`

**Implementation Pattern:**
```typescript
// In WebSocketManager.handleMessage()
case 'remote_execution_request':
  await this.executeRemoteCommand(message.data);
  break;

private async executeRemoteCommand(request: RemoteExecutionRequest) {
  // Execute command (e.g., screenshot)
  // Send response back with requestId correlation
}
```

### **Phase 2: html2canvas Integration** (Priority: HIGH, Est: 1-2 hours)

**Files to Create/Modify:**
- `src/ui/continuum-browser-client/commands/ScreenshotExecutor.ts`

**Tasks:**
1. Dynamic loading of html2canvas library
2. Screenshot capture function implementation
3. Base64 image data return pipeline

**Implementation Pattern:**
```typescript
async function captureScreenshot(selector: string): Promise<string> {
  // Dynamically load html2canvas
  // Capture element or full page
  // Return base64 image data
}
```

### **Phase 3: Complete WebSocket Pipeline** (Priority: MEDIUM, Est: 1-2 hours)

**Files to Modify:**
- `src/commands/core/remote-command/RemoteCommand.ts`

**Tasks:**
1. Complete `sendToClientViaWebSocket()` implementation
2. Add session-to-websocket routing
3. Implement timeout and error handling

**Implementation Pattern:**
```typescript
private static async sendToClientViaWebSocket(request, context) {
  // Find WebSocket connection for session
  // Send request to browser
  // Wait for response with timeout
  // Return result
}
```

### **Phase 4: Testing & Validation** (Priority: MEDIUM, Est: 1 hour)

**Tasks:**
1. Test screenshot with various CSS selectors
2. Validate file saving through existing DataMarshal
3. JTAG integration testing
4. Session management validation

## 🏗️ **Current Architecture (Working Parts)**

### ✅ **What Works:**
- Screenshot command registration and discovery
- Parameter parsing and validation (`ScreenshotCommand.ts`)
- File saving via DataMarshal and FileWrite commands
- WebSocket connection and basic messaging
- Session management and routing

### ❌ **What's Missing:**
- Browser cannot receive and execute server commands
- No html2canvas integration for actual screenshot capture
- Incomplete bidirectional WebSocket command pipeline

## 🔍 **File Structure Analysis**

### **Core Command Files:**
```
src/commands/browser/screenshot/
├── ScreenshotCommand.ts      # ✅ Command definition complete
├── ScreenshotHandler.ts      # ✅ Parameter handling complete  
├── ScreenshotTypes.ts        # ✅ Type definitions complete
└── README.md                 # ✅ Documentation complete
```

### **Missing Browser Integration:**
```
src/ui/continuum-browser-client/
├── commands/                 # ❌ Missing command executors
│   └── ScreenshotExecutor.ts # ❌ Need html2canvas integration
└── connection/
    └── WebSocketManager.ts   # ❌ Missing remote command handler
```

## 🎯 **Success Criteria**

When complete, this should work:
```bash
./continuum screenshot --filename=test.png --selector=.main-content
```

**Expected Flow:**
1. CLI → HTTP API → CommandProcessor
2. CommandProcessor → RemoteCommand.sendToClientViaWebSocket()
3. Server → Browser via WebSocket (`remote_execution_request`)
4. Browser executes html2canvas screenshot capture
5. Browser → Server via WebSocket (`remote_execution_response`)
6. Server saves image file via DataMarshal
7. CLI receives success response

## 📋 **Dependencies**

### **External Libraries:**
- `html2canvas` - For browser screenshot capture
- Existing WebSocket infrastructure
- DataMarshal file saving system

### **Internal Systems:**
- Session management (✅ Working)
- WebSocket daemon communication (✅ Working)
- Command discovery and routing (✅ Working)

## 🚀 **Next Steps**

1. **Phase 1**: Implement browser message handler for `remote_execution_request`
2. **Phase 2**: Add html2canvas dynamic loading and screenshot execution
3. **Phase 3**: Complete server-side WebSocket pipeline
4. **Phase 4**: End-to-end testing and validation

**Total Estimated Effort: 5-8 hours** to get screenshots fully operational.

---

*This plan addresses the core architecture gap preventing screenshot functionality while leveraging the existing, well-designed RemoteCommand infrastructure.*