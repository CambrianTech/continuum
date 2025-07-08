# BrowserWebSocketDaemon

**Phase 3 Browser Daemon** - WebSocket connection management for real-time communication

## 🎯 Purpose

Extracts WebSocket connection, message handling, and reconnection logic from monolithic `continuum-browser.ts` into modular daemon architecture. Handles 15% of the original monolithic code.

## 🏗️ Architecture

```typescript
// Main WebSocket daemon class
BrowserWebSocketDaemon extends BaseBrowserDaemon
├── Connection Management: establish, maintain, monitor WebSocket connections
├── Message Handling: send/receive with queueing and timeout management  
├── Reconnection Logic: exponential backoff with configurable retry limits
├── Session Coordination: automatic session establishment and management
├── Event System: subscribe/unsubscribe to WebSocket and connection events
└── Error Recovery: graceful handling of connection failures and timeouts
```

## 🔌 Responsibilities

### **Connection Management**
- WebSocket connection establishment and lifecycle management
- Connection state monitoring (`connecting`, `connected`, `disconnected`, `error`)
- Health checking and connection validation
- Graceful connection termination

### **Message Handling** 
- Reliable message sending with queueing for offline scenarios
- Message parsing and routing to appropriate handlers
- Request/response correlation with timeout management
- Command execution through WebSocket protocol

### **Reconnection Logic**
- Automatic reconnection with exponential backoff
- Configurable retry limits and delay intervals
- Connection state persistence across reconnection attempts
- Graceful degradation when max attempts reached

### **Session Coordination**
- Automatic session establishment on connection
- Client ID and session ID management
- Session state synchronization with server
- Integration with session management daemons

## 📡 Message Types

### **Incoming Messages**
```typescript
'websocket:connect'           // Establish WebSocket connection
'websocket:disconnect'        // Close WebSocket connection  
'websocket:send'             // Send message through WebSocket
'websocket:status'           // Get connection status and metrics
'websocket:subscribe'        // Subscribe to WebSocket events
'websocket:unsubscribe'      // Unsubscribe from WebSocket events
'websocket:execute_command'  // Execute command through WebSocket
```

### **Outgoing Events**
```typescript
'websocket:connecting'       // Connection attempt started
'websocket:connected'        // Connection established successfully
'websocket:disconnected'     // Connection closed (with reason)
'websocket:error'           // Connection error occurred
'websocket:max_reconnect_attempts' // Reconnection limit reached
'session:ready'             // Session established and ready
'message'                   // Raw message received
'message:{type}'            // Typed message received
```

## 🔧 Configuration

```typescript
interface ConnectionOptions {
  wsUrl?: string;                    // WebSocket URL (default: ws://localhost:9000)
  maxReconnectAttempts?: number;     // Max reconnection attempts (default: 5)
  reconnectDelay?: number;           // Base reconnection delay (default: 1000ms)
  connectionTimeout?: number;        // Connection timeout (default: 5000ms)
}
```

## 🎮 Usage

### **Basic Connection**
```typescript
// Through BrowserDaemonController
await daemonController.websocketDaemon.handleMessage({
  type: 'websocket:connect',
  data: { wsUrl: 'ws://localhost:9000' }
});

// Direct instantiation
const websocketDaemon = new BrowserWebSocketDaemon({
  maxReconnectAttempts: 10,
  reconnectDelay: 2000
});
await websocketDaemon.start();
```

### **Command Execution**
```typescript
// Execute command through WebSocket
const result = await websocketDaemon.executeCommand('health', {});

// Send raw message
await websocketDaemon.handleMessage({
  type: 'websocket:send',
  data: {
    type: 'custom_message',
    data: { key: 'value' }
  }
});
```

### **Event Subscription**
```typescript
// Subscribe to connection events
websocketDaemon.on('websocket:connected', (data) => {
  console.log('Connected with client ID:', data.clientId);
});

// Subscribe to specific message types
websocketDaemon.on('message:session_ready', (sessionData) => {
  console.log('Session ready:', sessionData.sessionId);
});
```

## 🧪 Testing

### **Unit Tests**
- Connection establishment and termination
- Message sending and receiving
- Reconnection logic with various failure scenarios
- Event subscription and unsubscription
- Command execution with timeout handling

### **Integration Tests**
- WebSocket server integration
- Session establishment flow
- Multi-daemon coordination
- Error recovery scenarios
- Performance under message load

## 🔄 Integration Points

### **With Console Daemon**
- Console logs sent through WebSocket connection
- Connection status affects console forwarding
- Shared session context for log correlation

### **With Session Manager**
- Session ID coordination and synchronization
- Session state changes trigger WebSocket events
- Session-based message routing

### **With Command Processor**
- Command execution through WebSocket protocol
- Command response correlation and timeout handling
- Command result forwarding to appropriate handlers

## 🚀 Migration Strategy

### **Phase 3 Implementation**
1. ✅ Extract WebSocket logic from `continuum-browser.ts`
2. ✅ Create modular daemon with clean message interface
3. ✅ Implement dual pattern with feature flag control
4. 📋 Integrate with BrowserDaemonController
5. 📋 Test WebSocket daemon with live server connection
6. 📋 Validate command execution and session establishment

### **Feature Flag Control**
```typescript
// Enable WebSocket daemon testing
localStorage.setItem('continuum_websocket_daemon', 'true');

// Check if WebSocket daemon is active
BrowserFeatureFlags.WEBSOCKET_DAEMON_ENABLED; // true/false
```

### **Backward Compatibility**
- Legacy WebSocket implementation remains as fallback
- Feature flag controls which implementation is used
- Gradual migration with easy rollback capability
- No breaking changes to existing WebSocket API

## 🔮 Future Enhancements

### **Phase 8: Web Worker Migration**
- Move WebSocket daemon to Web Worker for better performance
- Implement main thread proxy for DOM integration
- Maintain same message interface for seamless migration

### **Advanced Features**
- Connection pooling for multiple WebSocket connections
- Message compression and batching
- Connection quality monitoring and adaptive timeouts
- Automatic server endpoint discovery and failover

## 📊 Metrics

### **Connection Metrics**
- Connection establishment time
- Reconnection frequency and success rate
- Message send/receive throughput
- Connection stability over time

### **Performance Metrics**
- Message queue size and processing time
- Memory usage for buffered messages
- CPU usage for message processing
- Network bandwidth utilization

## 🛡️ Error Handling

### **Connection Errors**
- Network connectivity issues
- Server unavailability
- Authentication failures
- Protocol version mismatches

### **Message Errors**
- Malformed message parsing
- Command execution failures
- Timeout handling
- Queue overflow protection

### **Recovery Strategies**
- Exponential backoff reconnection
- Message queue persistence
- Graceful degradation modes
- User notification of connection issues