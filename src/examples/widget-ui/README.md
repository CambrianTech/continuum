# JTAG Widget-UI Development Environment

A standalone widget development environment that connects to the main JTAG system for testing widget functionality with improved TypeScript safety.

## 🎯 Purpose

- **Widget Development**: Focused environment for building and testing JTAG widgets
- **Type-Safe Testing**: Uses the latest JTAG package with improved TypeScript typing  
- **Isolated UI**: Runs on port 9003, connects to main JTAG system on port 9001
- **Real-time Testing**: Live widget updates with JTAG command execution

## 🏗️ Architecture

```
Widget-UI (Port 9003)          JTAG System (Port 9001)
├── HTTP Server                ├── WebSocket Server  
├── Static File Serving        ├── Command Daemons
├── Widget HTML/CSS/JS         ├── Transport Layer
└── Browser Bundle             └── Message Routing
         │                              │
         └─── WebSocket Connection ─────┘
```

## 🚀 Quick Start

### Prerequisites
1. Main JTAG system should be running on port 9001
2. Run from JTAG root directory: `npm run system:start`

### Deploy Widget-UI
```bash
# Option 1: Use deployment script
./scripts/deploy-widget-ui.sh

# Option 2: Manual deployment  
cd examples/widget-ui
npm install
npm start
```

### Access Points
- **Widget-UI**: http://localhost:9003
- **JTAG WebSocket**: ws://localhost:9001  
- **Chat Widget**: Available in browser at localhost:9003

## 🎭 Widget Development

### Current Widgets
- `<chat-widget>`: Chat interface with JTAG command routing
- More widgets can be added following the same pattern

### Widget Architecture
```javascript
// All widgets use the unified widget daemon for command execution
const result = await this.executeCommand('screenshot', { 
  querySelector: '#target' 
});
```

### Type Safety Improvements  
- ✅ `CommandParams` properly typed with required context/sessionId
- ✅ `CommandResult` enforces proper response structure
- ✅ Widget event handlers use `CustomEvent<ChatMessageEventData>`
- ✅ No more `any` types in critical widget interfaces

## 🔧 Configuration

### Ports
- **Widget-UI HTTP**: 9003 (configurable in package.json)
- **JTAG WebSocket**: 9001 (connects to main system)

### Package Dependencies
- `@continuum/jtag`: file:../../continuum-jtag-1.0.1050.tgz
- Latest package with TypeScript improvements

## 🧪 Testing

### Widget Command Testing
```javascript
// Test widget command execution
const pingResult = await widgetDaemon.executeCommand('ping', {});
const screenshot = await widgetDaemon.executeCommand('screenshot', {
  querySelector: 'chat-widget'
});
```

### Browser Console Testing  
- Open browser dev tools at localhost:9003
- All widget commands route through JTAG system
- Console logs show command execution flow

## 🐛 Troubleshooting

### Common Issues
1. **Port 9003 in use**: Run `npx tsx ../../scripts/cleanup-dynamic-ports.ts`
2. **JTAG system not running**: Start with `npm run system:start` from JTAG root
3. **TypeScript errors**: Widget-UI uses latest package with strict typing
4. **WebSocket connection failed**: Verify main JTAG system is on port 9001

### Debug Commands
```bash
# Check port usage
lsof -i :9001 -i :9003

# View widget logs
tail -f .continuum/jtag/currentUser/logs/browser.log

# Test WebSocket connection
curl -i -N -H "Connection: Upgrade" -H "Upgrade: websocket" http://localhost:9001
```

## 📋 Development Workflow

1. **Start JTAG System**: `npm run system:start` (from root)
2. **Start Widget-UI**: `npm start` (from examples/widget-ui)
3. **Open Browser**: http://localhost:9003
4. **Develop Widgets**: Edit widget files and refresh
5. **Test Commands**: Use browser console or widget UI
6. **Debug Issues**: Check logs in .continuum/jtag/logs/

This environment provides a safe, isolated space for widget development while maintaining full integration with the JTAG system's powerful command routing and type-safe interfaces.