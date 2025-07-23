# JTAG Examples - Simple Architecture Demo

## 🏗️ **Simple Architecture: Just Include and Use**

This examples directory demonstrates the **simple JTAG architecture** where applications just include the script and use the API or console - no complex setup needed.

## 🎯 **Architecture Overview**

### **Server-Side (simple-app.ts)**
```typescript
// ONE LINE - that's it! Console auto-attached, full API available
import { jtag } from '../index';

// Your normal app code - console automatically logged to .continuum/jtag/logs/
console.log('Hello from simple app!');
console.error('Test error message');

// Use JTAG API directly when needed  
jtag.critical('APP', 'Application started', { version: '1.0.0' });
let image = await jtag.screenshot('app-startup.png');
```

### **Browser-Side (demo.html + demo.js)**  
```html
<!-- Include JTAG - handles everything automatically -->
<script src="/jtag.js"></script>

<script>
  function testLogging() {
    // Use JTAG API directly
    jtag.log('BROWSER', 'Browser logging via JTAG API');
    
    // Or just use console (automatically intercepted)
    console.log('This console.log is automatically sent to server');
    console.error('Errors are captured too');
    // → Both appear in .continuum/jtag/logs/browser.log
  }
</script>
```

### **Example App Buttons (All Console/JTAG API Calls)**

The demo page buttons are simple:
- **"Test Browser Logging"** → `jtag.log()` and `console.log()` calls
- **"Test Browser Screenshot"** → `let image = await jtag.screenshot()`
- **"Test Cross-Context"** → `jtag.critical()` and `console.error()` calls

**All output appears in `.continuum/jtag/logs/` files automatically.**

## 📦 **Error Handling Patterns**

### **Await Pattern (Recommended)**
```javascript
// Clean async/await with dual error handling
async function takeScreenshot() {
  try {
    let image = await jtag.screenshot('demo-capture', {
      selector: '#my-component',
      width: 800,
      height: 600
    });
    
    if (image.success) {
      console.log(`✅ Saved: ${image.filepath}`);
      logToBrowser(`📸 Screenshot saved: ${image.filepath}`);
    } else {
      console.log(`❌ Failed: ${image.error}`);
      logToBrowser(`📸 Screenshot failed: ${image.error}`);
      // Expected failures: no browser, invalid selector, permissions, etc.
    }
  } catch (error) {
    console.log(`💥 Exception: ${error.message}`);
    logToBrowser(`💥 System error: ${error.message}`);
    // Unexpected failures: network down, system crash, etc.
  }
}
```

### **Promise Pattern (Alternative)**  
```javascript
// Promise chains with dual error handling
function takeScreenshot() {
  jtag.screenshot('demo-capture')
    .then(image => {
      if (image.success) {
        console.log(`✅ Saved: ${image.filepath}`);
      } else {
        console.log(`❌ Failed: ${image.error}`);
        // Handle expected failures
      }
    })
    .catch(error => {
      console.log(`💥 Exception: ${error.message}`);
      // Handle unexpected exceptions
    });
}
```

**Both patterns work - use whichever fits your flow!**

### **📁 Example Files**

#### **1. standalone-integration.js**
Complete end-to-end integration test that runs both client and server sides independently.

```bash
# Run complete integration test
node examples/standalone-integration.js

# Expected output:
🚨 JTAG Standalone Integration Test
====================================
📡 Testing Server-Side JTAG...
   ✅ Server-side JTAG operational
🌐 Testing Client-Side JTAG...
   📤 Client request 0: SUCCESS  
   📦 Payload screenshot_data: SUCCESS
📊 Integration Test Results
🎉 JTAG Integration Test PASSED
```

**What it tests:**
- Server-side JTAG initialization and HTTP server startup
- Client-side HTTP requests to JTAG server endpoints
- Screenshot functionality (server-side placeholders)  
- Base64 payload transport (screenshot data, log batches)
- File system operations (log creation, directory structure)
- Cross-context communication via port 9001

#### **2. simple-usage.js**
Demonstrates basic JTAG usage patterns for developers.

```bash
# Run simple usage examples  
node examples/simple-usage.js

# Shows:
📝 Basic logging patterns
📊 Different log levels (log, critical, trace, probe)
⚙️ Configuration access
📸 Screenshot usage (async and fire-and-forget)
🐛 Error debugging patterns
⏱️ Performance monitoring
```

#### **3. browser-simulation.html**
Interactive HTML page that simulates browser-side JTAG usage with real UI.

```bash
# Serve the HTML file
python3 -m http.server 8080
# or
npx serve examples/

# Open http://localhost:8080/browser-simulation.html
# Interactive buttons test all JTAG features
```

**Browser features tested:**
- Visual JTAG logging with console output
- Screenshot requests to server (with selector support)
- Base64 payload transport from browser
- Real HTTP communication to port 9001
- Error simulation and recovery patterns
- Live statistics display

### **🚀 Running Integration Tests**

#### **Complete Standalone Test**
```bash
# 1. Navigate to JTAG module
cd src/debug/emergency-jtag

# 2. Run standalone integration (tests everything)
node examples/standalone-integration.js

# 3. Check created files
ls -la .continuum/jtag/logs/       # Server logs
ls -la .continuum/jtag/screenshots/ # Screenshot files
```

#### **Browser + Server Test**
```bash
# Terminal 1: Start server-side (via integration test)
node examples/standalone-integration.js

# Terminal 2: Serve browser simulation  
cd examples && python3 -m http.server 8080

# Browser: Open http://localhost:8080/browser-simulation.html
# Click buttons to test browser ↔ server communication
```

#### **Development Testing**
```bash
# Quick usage examples
node examples/simple-usage.js

# Run unit tests
node tests/test-runner.js

# Full validation (unit + integration)  
npm test -- src/debug/emergency-jtag/
```

### **📊 Test Coverage Matrix**

| Feature | Unit Tests | Integration | Browser Sim |
|---------|------------|-------------|-------------|
| **Configuration** | ✅ | ✅ | ✅ |
| **Basic Logging** | ✅ | ✅ | ✅ |
| **Log Levels** | ✅ | ✅ | ✅ |
| **Screenshots** | ✅ | ✅ | ✅ |
| **Base64 Transport** | ❌ | ✅ | ✅ |
| **Cross-Context** | ❌ | ✅ | ✅ |
| **File System** | ✅ | ✅ | ❌ |
| **HTTP Server** | ❌ | ✅ | ✅ |
| **Error Handling** | ✅ | ✅ | ✅ |

### **🔧 Test Configuration**

#### **Integration Test Config**
```javascript
const INTEGRATION_CONFIG = {
  testDuration: 5000,    // 5 seconds
  serverPort: 9001,     // JTAG server port  
  clientRequests: 10,   // Number of client requests
  screenshotTests: 3,   // Number of screenshot tests
  payloadTests: 2       // Number of payload tests
};
```

#### **Expected File Structure After Tests**
```
.continuum/jtag/
├── logs/
│   ├── server.log.log          # Server general logs
│   ├── server.critical.log     # Server critical events
│   ├── server.trace.log        # Server function traces
│   ├── server.probe.log        # Server state probes
│   ├── server.all.log          # All server logs combined
│   ├── browser.log.log         # Browser logs (from client tests)
│   └── browser.all.log         # All browser logs
└── screenshots/
    ├── server-integration-test.png.txt  # Server screenshot placeholder
    ├── browser-payload-*.png            # Base64 screenshot data
    └── server-example-*.png.txt         # Example screenshots
```

### **🎯 Success Criteria**

#### **Integration Test Success**
- ✅ Server JTAG starts successfully on port 9001
- ✅ Client requests receive 200 responses  
- ✅ Screenshots return success results
- ✅ Base64 payloads process correctly
- ✅ Log files created with expected naming
- ✅ Overall success rate ≥ 80%

#### **Browser Simulation Success**
- ✅ All buttons trigger JTAG calls
- ✅ Console shows proper JTAG output
- ✅ Server communication works (no CORS errors)
- ✅ Screenshots return results
- ✅ Statistics display correctly

### **🐛 Debugging Failed Tests**

#### **Server Won't Start**
```bash
# Check port availability
lsof -i :9001

# Check JTAG config
node -e "console.log(require('./shared/config').jtagConfig)"

# Manual server test
node -e "require('./EmergencyJTAG').jtag.log('TEST', 'Manual test')"
```

#### **Client Requests Fail**  
```bash
# Test JTAG server directly
curl -X POST http://localhost:9001/jtag \
  -H "Content-Type: application/json" \
  -d '{"timestamp":"2025-01-01T00:00:00.000Z","context":"test","component":"CURL","message":"test","type":"log"}'

# Check server logs
tail -f .continuum/jtag/logs/server.all.log
```

#### **Files Not Created**
```bash
# Check permissions
ls -la .continuum/

# Check directory creation
node -e "console.log(require('fs').existsSync('.continuum/jtag/logs'))"

# Manual file test
node examples/simple-usage.js && ls .continuum/jtag/logs/
```

### **🔮 Extended Testing**

#### **Performance Testing**
```bash
# Stress test with many requests
node -e "
const {jtag} = require('./EmergencyJTAG');
for(let i=0; i<1000; i++) jtag.log('STRESS', 'Message '+i);
"
```

#### **Concurrent Testing**  
```bash
# Run multiple integration tests simultaneously
node examples/standalone-integration.js &
node examples/standalone-integration.js &
wait
```

#### **Memory Testing**
```bash
# Monitor memory usage during tests
node --inspect examples/standalone-integration.js
# Use Chrome DevTools to profile memory
```

---

**The examples directory provides complete standalone testing that validates JTAG can debug any system, even when that system is completely broken.** 🚨