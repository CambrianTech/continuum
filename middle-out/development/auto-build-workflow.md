# Auto-Build & Version System Workflow

## 🎯 Automated Build Integration

### **Commands That Auto-Build**

All primary development commands now automatically increment version and rebuild browser JS:

```bash
# All these commands auto-rebuild with version increment:
npm run dev          # Auto-build → Start development mode
npm run launch       # Auto-build → Start full system  
npm start:full       # Auto-build → Start with all features
./continuum          # Uses launch internally → auto-builds
```

### **Build Process Flow**

```
npm run [command] 
    ↓
npm run version:bump  (increments package.json version)
    ↓ 
esbuild src/ui/continuum-browser.ts → src/ui/continuum-browser.js
    ↓
Version embedded in JS file from package.json import
    ↓
System starts with fresh browser JS
```

## 🔍 Version Verification Workflow

### **1. Run System**
```bash
./continuum  # Auto-builds and increments version
```

### **2. Check Browser Version**
```bash
# Version embedded in downloaded JS file:
curl -s http://localhost:9000/dist/ui/continuum-browser.js | grep -o 'version: ".*"'

# Version in window object:
curl -s http://localhost:9000 | grep -o 'CONTINUUM_VERSION.*'
```

### **3. Check Package Version**
```bash
cat package.json | grep '"version"'
```

### **4. Find All Version References**
```bash
# Search for current version across system:
grep -r "version.*0\.2\.2274" . --exclude-dir=node_modules

# Find version in logs:
find .continuum -name "*.log" -exec grep -l "version" {} \;
```

## 📝 Browser Log Infrastructure

### **Session Files Auto-Created**
```bash
# Every launch creates session files:
.continuum/sessions/user/shared/[SESSION_ID]/logs/browser.log
.continuum/sessions/user/shared/[SESSION_ID]/logs/server.log
```

### **Console Command Testing**
```bash
# Test console logging (session context pending):
curl -X POST http://localhost:9000/api/commands/console \
  -H "Content-Type: application/json" \
  -d '{"action": "log", "message": "UUID test message", "source": "browser"}'
```

### **UUID Logging Pattern**
```bash
# For JTAG debugging, use this pattern:
console.log('🎯 EXECUTION_UUID_' + Date.now() + '_START');
# ... your code ...
console.log('🎯 EXECUTION_UUID_' + Date.now() + '_COMPLETE');
```

## 🎯 Development Workflow Integration

### **Middle-Out Layer Testing with Auto-Build**

```bash
# Layer 1: Foundation (auto-builds)
./continuum && curl -s http://localhost:9000/api/commands/health

# Layer 2: Commands (auto-builds)  
./continuum && npm run test:commands

# Layer 3: Integration (auto-builds)
./continuum && npm run test:integration
```

### **Version Tracking in Development**

Every development session now automatically:
1. **Increments version** - Tracks development iterations
2. **Rebuilds browser JS** - Ensures latest code deployed
3. **Embeds version** - Browser gets correct version info
4. **Creates session logs** - Ready for JTAG debugging

## 🔧 Troubleshooting

### **Version Mismatch Issues**
```bash
# If browser shows old version:
1. Kill all processes: killall node
2. Restart: ./continuum  
3. Verify: curl -s http://localhost:9000/dist/ui/continuum-browser.js | grep version
```

### **Build Failures**
```bash
# Manual build if auto-build fails:
npm run build:browser-ts

# Check build output:
ls -la src/ui/continuum-browser.js
```

This automated system ensures consistent version tracking across the entire development lifecycle.