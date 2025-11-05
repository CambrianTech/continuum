# 🔥 Hot Reload Development Workflow

**Autonomous AI Development with Session Preservation**

## Overview

The Hot Reload system solves the classic webpack problem for autonomous AI development: how to see code changes instantly without losing valuable debugging session state and logs.

## 🎯 **The Problem Solved**

**Before**: `npm start` → Clean sessions → Lose debugging context → Restart everything
**After**: `./jtag watch` → File changes → Auto rebuild + reload → Session preserved

## 🚀 **Commands**

### **Manual Hot Reload**
```bash
./jtag hot-reload
```
- Rebuilds browser bundle without cleaning sessions
- Reloads browser page while preserving WebSocket connections
- Maintains all logs, session state, and debugging context

### **Automatic File Watching**
```bash
./jtag watch
```
- Monitors `src/ui/components/**/*.ts` for changes
- Auto-triggers hot reload on file modifications
- 1-second debounce to handle rapid changes
- Press Ctrl+C to stop

### **Development Scripts**
```bash
# Session-preserving rebuild (used by hot reload)
npm run build:browser-hot

# Full rebuild with session cleanup (original)
npm run build:browser-ts
```

## 🏗️ **Architecture**

### **Modular Design**
```
jtag.ts                     # Main JTAG CLI - focused on debugging
├── ./jtag hot-reload      # Manual rebuild + reload
├── ./jtag watch           # Launches file watcher
└── ./jtag warnings        # Debug deprecated APIs

src/hot-reload.ts          # Dedicated file watcher module
├── File polling (no deps) # Native Node.js approach
├── Smart debouncing       # Handles rapid changes
└── Error handling         # Graceful failure recovery
```

### **Session Preservation Flow**
1. **File Change Detected** → `src/ui/components/Widget.ts`
2. **Debounced Trigger** → Wait 1 second for more changes
3. **Rebuild Bundle** → `npm run build:browser-hot` (no session cleanup)
4. **Reload Browser** → API call to `/api/commands/reload`
5. **Session Maintained** → Logs, WebSocket, debugging state preserved

## 🔧 **Implementation Details**

### **File Watching Strategy**
- **Native polling** instead of external dependencies (chokidar)
- **2-second intervals** to check file modification times
- **Efficient scanning** using `find` command for file discovery
- **Change detection** via `mtime` comparison

### **Build Process**
```bash
# Hot reload build (session-preserving)
npm run version:bump &&         # Track build versions
rm -f src/ui/continuum-browser.js* &&  # Clear old bundle
node scripts/build-browser.cjs   # Build new bundle

# vs Original build (session-destroying)  
npm run clean:all &&            # ❌ Destroys sessions
npm run version:bump &&
rm -f src/ui/continuum-browser.js* &&
node scripts/build-browser.cjs
```

### **Browser Reload**
```typescript
// Preserves WebSocket connections and session state
await fetch('http://localhost:9000/api/commands/reload', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ target: 'page' })
});
```

## 📊 **Benefits**

### **For Autonomous AI Development**
- **Instant feedback** on widget changes
- **Preserved debugging context** - no lost session state
- **Continuous monitoring** via JTAG warnings/errors
- **Real-time development** without restart overhead

### **For Human Development**
- **Webpack-like experience** without webpack complexity
- **Session continuity** - maintain debug breakpoints, logs
- **Rapid iteration** on UI components
- **Zero-config setup** - works out of the box

## 🛠️ **Usage Patterns**

### **AI Widget Debugging**
```bash
# Start development session
npm start

# Begin autonomous debugging with hot reload
./jtag watch

# In another terminal: monitor for issues
./jtag warnings
./jtag errors

# AI modifies widget code → automatic reload → check results
```

### **Human Development**
```bash
# Traditional development
./jtag watch

# Make code changes → see results instantly
# Session state preserved throughout
```

## 🚨 **Troubleshooting**

### **File Watcher Not Triggering**
- Check file permissions on `src/ui/components/`
- Verify TypeScript files are being modified (not just opened)
- Watch the console for "Changes detected" messages

### **Build Failures**
- TypeScript compilation errors will prevent hot reload
- Check `npm run compile` first to fix TypeScript issues
- Build output shows specific error details

### **Browser Not Reloading**
- Ensure system is running (`./jtag health`)
- Check WebSocket connection is active
- Verify browser is connected to localhost:9000

## 🔮 **Future Enhancements**

### **Planned Features**
- **CSS hot reload** - Update styles without full page reload
- **Selective component refresh** - Reload only changed widgets
- **Error overlay** - Show build errors in browser
- **Performance metrics** - Track reload times and optimization

### **Integration Opportunities**
- **Academy training** - Feed hot reload patterns to AI personas
- **JTAG analysis** - Correlate code changes with widget behavior
- **Automated testing** - Trigger tests after successful reloads

## 📚 **Related Documentation**

- **[JTAG Debugging](../jtag/README.md)** - Complete JTAG system overview
- **[Widget Development](../ui/widget-development.md)** - Widget creation patterns
- **[Session Management](../sessions/session-lifecycle.md)** - Session preservation details
- **[Build System](../build/browser-bundling.md)** - Build process architecture

---

**Key Insight**: Hot reload transforms autonomous AI development from a stop-start process into a continuous feedback loop, enabling true AI-driven iteration without losing debugging context.