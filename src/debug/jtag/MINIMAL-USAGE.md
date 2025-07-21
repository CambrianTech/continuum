# 🚨 JTAG - Minimal Usage

**One line setup. Zero configuration. Console auto-attached. Full API available.**

## Server Side (Node.js)

```javascript
// ONE LINE - that's it!
const { jtag } = require('./src/debug/jtag');

// Console automatically attached + full API available
console.log('Auto-logged to files');
jtag.screenshot('debug.png');  // Full API ready
jtag.critical('DB', 'Connection failed', errorData);
```

## Browser Side (TypeScript/JS)

```typescript
// ONE LINE - that's it!
import { jtag } from './src/debug/jtag';

// Console automatically attached + full API available  
console.log('Auto-logged to server files via WebSocket');
jtag.screenshot('browser-debug.png');  // Full API ready
jtag.exec('window.innerWidth');  // Code execution
```

## What Happens Automatically

✅ **Console Routing**: `console.log()` → both console + log files  
✅ **WebSocket Server**: Auto-starts on port 9001  
✅ **File Logging**: All logs saved to `.continuum/jtag/logs/`  
✅ **Cross-Context**: Browser logs → Server files via WebSocket  
✅ **Strong Typing**: Full TypeScript support  

## Log File Structure

```
.continuum/jtag/logs/
├── server.log      # All server console output
├── server.info.log # console.log() only
├── server.error.log# console.error() only
├── browser.log     # All browser console output
└── browser.info.log# Browser console.log() only
```

## Examples

**Server App:**
```bash
npx tsx examples/simple-app.ts  # Clean TypeScript app with JTAG
```

**Browser App:**
```bash
# Serve the HTML file and use TypeScript imports
# All console output automatically routed to server log files
```

**That's it!** 
- ✅ Import once, console auto-attached
- ✅ Full JTAG API available (`jtag.screenshot()`, `jtag.critical()`, etc.)
- ✅ Zero configuration, zero complexity
- ✅ All WebSocket/file handling internal to the package

## 🔗 Integration Test Chain

**Key Insight**: Example apps become **live test harnesses** by properly using the JTAG API.

When you run `npm start`, the example app:
1. **Auto-wires JTAG** via single import
2. **Creates WebSocket server** for browser connections  
3. **Enables integration tests** to validate real cross-context communication
4. **Proves the system works** by using it correctly

Your integration tests can now interact with a **real JTAG-enabled application** rather than mocks, providing true end-to-end validation.

See `INTEGRATION-TEST-CHAIN.md` for complete details.