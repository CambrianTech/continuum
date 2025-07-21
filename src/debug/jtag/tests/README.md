# JTAG Testing System - Middle-Out Layer Architecture

## 🧅 Layer-Based Testing Structure

Following the middle-out testing methodology, JTAG tests are organized by validation layers that must pass in sequence:

### **Layer 1: Foundation Tests** - `layer-1-foundation/`
**Prerequisites**: TypeScript compilation passes, core types load
- `JTAGWebSocket.simple.test.ts` - WebSocket basic functionality
- `JTAGWebSocket.test.ts` - WebSocket comprehensive testing  
- `console-mapping.test.ts` - Console interception and mapping validation

**Success Criteria**: 
- ✅ WebSocket connections establish successfully
- ✅ Console methods map correctly (console.error → jtag.error) 
- ✅ Base message transport works

### **Layer 2: Daemon Processes** - `layer-2-daemon-processes/`
**Prerequisites**: Layer 1 passes
- `logging-system-integration.test.ts` - End-to-end log file creation
- `websocket-server-integration.test.ts` - Server daemon startup and operation

**Success Criteria**:
- ✅ JTAG server starts on configured port (9001)
- ✅ Log files created with correct platform.level.txt pattern
- ✅ WebSocket server accepts and processes messages

### **Layer 4: System Integration** - `layer-4-system-integration/`
**Prerequisites**: Layers 1-2 pass
- `jtag-integration.test.ts` - Full JTAG system integration
- `websocket-integration.test.ts` - Client-server message flow
- `jtag-real-integration.test.ts` - Real-world scenario testing
- `module-integration-test.ts` - Module loading and API testing
- `standalone-integration-test.ts` - Standalone system validation
- `screenshot-integration.test.ts` - Screenshot functionality testing

**Success Criteria**:
- ✅ Browser ↔ Server communication works end-to-end
- ✅ Screenshot requests process correctly
- ✅ Log entries persist to files with correct format
- ✅ Module can run independently of continuum system

### **Layer 6: Browser Integration** - `layer-6-browser-integration/`
**Prerequisites**: Layers 1,2,4 pass
- `browser-automation-test.ts` - Puppeteer browser automation
- `integration-with-browser-open.ts` - Live browser testing
- `manual-browser-test.ts` - Interactive browser validation

**Success Criteria**:
- ✅ Browser loads JTAG correctly via script tag
- ✅ Console interception works in real browser context
- ✅ Screenshots capture actual browser content
- ✅ WebSocket connection stable in browser environment

### **Examples as Integration Tests** - `examples/`
- `browser-simulation.html` - Interactive browser testing interface
- **Used by integration tests** - Examples ARE the integration validation

**Success Criteria**:
- ✅ Standalone HTML page runs JTAG completely independently
- ✅ All buttons trigger correct JTAG functionality
- ✅ Server communication works without CORS issues
- ✅ Visual validation demonstrates working system

## 🔧 Test Execution Strategy

### **Sequential Layer Testing** (REQUIRED ORDER)
```bash
# Layer 1: Foundation
npm run test:layer-1

# Layer 2: Daemon Processes  
npm run test:layer-2

# Layer 4: System Integration (skip layer 3 for JTAG)
npm run test:layer-4

# Layer 6: Browser Integration
npm run test:layer-6

# Full validation
npm run test:all
```

### **Individual Test Categories**
```bash
# WebSocket-specific tests
npm run test:websocket:all

# Browser automation (Puppeteer)  
npm run test:browser

# Standalone validation
npm run test:standalone

# Module integration
npm run test:module

# Manual interactive testing
npm run test:manual
```

### **Integration with Continuum Tests**
```bash
# From continuum root - includes JTAG in full test suite
npm test

# Test JTAG module specifically from continuum root
npm test -- src/debug/jtag/

# Independent JTAG testing (can run without continuum)
cd src/debug/jtag && npm test
```

## 📊 Test Coverage Matrix

| Feature | Layer 1 | Layer 2 | Layer 4 | Layer 6 | Examples |
|---------|---------|---------|---------|---------|----------|
| **WebSocket Connection** | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Console Mapping** | ✅ | - | ✅ | ✅ | ✅ |
| **Log File Creation** | - | ✅ | ✅ | - | - |
| **Screenshot API** | - | - | ✅ | ✅ | ✅ |  
| **Browser Integration** | - | - | - | ✅ | ✅ |
| **Standalone Operation** | - | - | ✅ | ✅ | ✅ |
| **Error Handling** | ✅ | ✅ | ✅ | ✅ | ✅ |

## 🎯 Debugging Failed Tests

### **Layer 1 Failures**
```bash
# Check TypeScript compilation
npx tsc --noEmit --project .

# Verify WebSocket functionality
node -e "console.log(require('ws').WebSocket)"

# Test console mapping directly
npx tsx tests/layer-1-foundation/console-mapping.test.ts
```

### **Layer 2 Failures** 
```bash
# Check port availability
lsof -i :9001

# Verify log directory permissions
ls -la ../../../.continuum/jtag/logs/

# Test daemon startup
npx tsx tests/layer-2-daemon-processes/logging-system-integration.test.ts
```

### **Layer 4 Failures**
```bash
# Check full integration
npx tsx tests/layer-4-system-integration/jtag-real-integration.test.ts

# Verify module loading
npx tsx tests/layer-4-system-integration/module-integration-test.ts

# Test screenshot functionality
npx tsx tests/layer-4-system-integration/screenshot-integration.test.ts
```

### **Layer 6 Failures**
```bash
# Test with Puppeteer
npx tsx tests/layer-6-browser-integration/browser-automation-test.ts

# Manual browser validation
npm run test:manual
# Open browser to http://localhost:8080 and test interactively
```

## 🚀 Dual Testing Capability

**JTAG supports both standalone and integrated testing:**

### **Standalone Testing (JTAG as independent NPM module)**
```bash
cd src/debug/jtag
npm test                    # Runs complete JTAG test suite independently
npm run test:all           # All layers + integration + browser tests  
npm start                  # Launches examples/end-to-end-demo.js
```

### **Continuum Integration Testing**
```bash
# From continuum root
npm test                   # Includes JTAG in full continuum test suite
npm test -- src/debug/jtag # Tests JTAG module as part of continuum
```

**Success Validation** (Both modes):
1. ✅ Check log files exist: `ls -la .continuum/jtag/logs/`
2. ✅ Verify examples work: Open `examples/browser-simulation.html`
3. ✅ Test standalone: `npm run test:standalone`
4. ✅ Full validation: `npm run test:all`

**The layer-based approach ensures each foundation is solid before building the next level, preventing cascade failures and providing clear debugging paths. JTAG can be tested completely independently or as part of the broader continuum system.**