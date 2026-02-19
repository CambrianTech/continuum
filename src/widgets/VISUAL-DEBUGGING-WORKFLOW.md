# Visual Debugging Workflow for Desktop Layout

## 🎯 **JTAG-POWERED VISUAL DEVELOPMENT**

The debugging approach: **Build → Screenshot → Exec → Iterate**

## 🔧 **CRITICAL: npm test FIRST**

### **MANDATORY: Test Validation After Every Change**
```bash
# BEFORE doing ANYTHING else - validate tests pass
npm test
# ☝️ THIS MUST PASS 100% - NO EXCEPTIONS

# ONLY if npm test passes, proceed with development:
JTAG_WORKING_DIR="examples/widget-ui" npm start
```

### **Essential Development Cycle**
```bash
# 1. Make small change (HTML/CSS/TS)
# 2. IMMEDIATELY validate
npm test                    # 🚨 MUST PASS - NO COMMITS IF THIS FAILS
npx tsc --noEmit           # Compile check
# 3. ONLY if tests pass, continue visual development
JTAG_WORKING_DIR="examples/widget-ui" npm start
# 4. Take screenshots to see what you built
./jtag interface/screenshot --querySelector=".desktop-container" --filename="desktop-current.png"
# 5. Execute JavaScript to test interactions  
./jtag exec --code="console.log('Testing interaction...')" --environment="browser"
```

## 📸 **SCREENSHOT DEBUGGING STRATEGY**

### **Progressive Screenshot Capture**
```bash
# Step 1: Basic structure
./jtag interface/screenshot --querySelector="body" --filename="01-basic-structure.png"

# Step 2: Left sidebar only
./jtag interface/screenshot --querySelector=".left-sidebar" --filename="02-left-sidebar.png"

# Step 3: Main panel only
./jtag interface/screenshot --querySelector=".main-panel" --filename="03-main-panel.png"

# Step 4: Full desktop layout
./jtag interface/screenshot --querySelector=".desktop-container" --filename="04-full-desktop.png"

# Step 5: Existing chat widget still working
./jtag interface/screenshot --querySelector="chat-widget" --filename="05-chat-widget-preserved.png"

# Step 6: Specific components
./jtag interface/screenshot --querySelector="continuum-emoter" --filename="06-emoter-component.png"
./jtag interface/screenshot --querySelector="dynamic-list" --filename="07-dynamic-list.png"
```

### **Before/After Comparison Workflow**
```bash
# Capture before making changes
./jtag interface/screenshot --querySelector=".desktop-container" --filename="before-css-changes.png"

# Make CSS/HTML changes

# Capture after changes
./jtag interface/screenshot --querySelector=".desktop-container" --filename="after-css-changes.png"

# Compare visually to see what changed
```

## ⚙️ **JAVASCRIPT EXEC DEBUGGING**

### **Testing Layout Structure**
```bash
# Check if elements exist
./jtag exec --code="
console.log('Desktop elements check:');
console.log('Desktop container:', !!document.querySelector('.desktop-container'));
console.log('Left sidebar:', !!document.querySelector('.left-sidebar'));
console.log('Chat widget:', !!document.querySelector('chat-widget'));
console.log('All elements found');
" --environment="browser"

# Check CSS grid layout
./jtag exec --code="
const container = document.querySelector('.desktop-container');
const computed = getComputedStyle(container);
console.log('Grid template columns:', computed.gridTemplateColumns);
console.log('Container dimensions:', container.getBoundingClientRect());
" --environment="browser"
```

### **Testing Component Interactions**
```bash
# Test dynamic list clicking
./jtag exec --code="
const academyItem = document.querySelector('[data-context=\"academy\"]');
if (academyItem) {
  academyItem.click();
  console.log('✅ Academy context clicked');
} else {
  console.log('❌ Academy item not found');
}
" --environment="browser"

# Test draggable separator
./jtag exec --code="
const separator = document.querySelector('.draggable-separator');
if (separator) {
  separator.dispatchEvent(new MouseEvent('mousedown', { clientX: 250 }));
  document.dispatchEvent(new MouseEvent('mousemove', { clientX: 300 }));
  document.dispatchEvent(new MouseEvent('mouseup'));
  console.log('✅ Separator drag test complete');
}
" --environment="browser"
```

### **Testing CSS Changes Live**
```bash
# Test theme colors
./jtag exec --code="
document.documentElement.style.setProperty('--color-primary-500', '#ff6b6b');
console.log('🎨 Changed primary color to coral');
" --environment="browser"

# Test layout adjustments
./jtag exec --code="
const container = document.querySelector('.desktop-container');
container.style.gridTemplateColumns = '300px 4px 1fr 4px 250px';
console.log('📐 Adjusted grid layout widths');
" --environment="browser"

# Capture result
./jtag interface/screenshot --querySelector=".desktop-container" --filename="live-css-test.png"
```

## 🐛 **COMMON DEBUGGING SCENARIOS**

### **1. Layout Not Appearing**
```bash
# Check if HTML structure exists
./jtag exec --code="
console.log('HTML structure check:');
const elements = {
  'desktop-container': document.querySelector('.desktop-container'),
  'left-sidebar': document.querySelector('.left-sidebar'),
  'main-panel': document.querySelector('.main-panel')
};
Object.entries(elements).forEach(([name, el]) => {
  console.log(\`\${name}: \${!!el} - \${el ? 'EXISTS' : 'MISSING'}\`);
});
" --environment="browser"

# Check CSS loading
./jtag exec --code="
const styles = Array.from(document.styleSheets).map(sheet => sheet.href || 'inline');
console.log('Loaded stylesheets:', styles);
" --environment="browser"
```

### **2. Components Not Registering**
```bash
# Check custom elements registration
./jtag exec --code="
const registeredElements = [
  'continuum-emoter',
  'dynamic-list', 
  'draggable-separator'
];
registeredElements.forEach(name => {
  const defined = customElements.get(name);
  console.log(\`\${name}: \${defined ? 'REGISTERED' : 'NOT REGISTERED'}\`);
});
" --environment="browser"
```

### **3. CSS Grid Issues**
```bash
# Debug grid layout
./jtag exec --code="
const container = document.querySelector('.desktop-container');
const computed = getComputedStyle(container);
console.log('CSS Grid Debug:');
console.log('Display:', computed.display);
console.log('Grid template columns:', computed.gridTemplateColumns);
console.log('Grid template rows:', computed.gridTemplateRows);
console.log('Container height:', computed.height);
" --environment="browser"
```

### **4. Chat Widget Still Working Check**
```bash
# Verify chat widget functionality preserved
./jtag exec --code="
const chatWidget = document.querySelector('chat-widget');
const input = chatWidget?.shadowRoot?.getElementById('messageInput');
const button = chatWidget?.shadowRoot?.getElementById('sendButton');
console.log('Chat widget check:');
console.log('Widget exists:', !!chatWidget);
console.log('Input exists:', !!input);
console.log('Button exists:', !!button);
console.log('Shadow root:', !!chatWidget?.shadowRoot);
" --environment="browser"

# Test chat widget functionality
./jtag exec --code="
const chatWidget = document.querySelector('chat-widget');
const input = chatWidget?.shadowRoot?.getElementById('messageInput');
const button = chatWidget?.shadowRoot?.getElementById('sendButton');
if (input && button) {
  input.value = 'Desktop layout test message';
  button.click();
  console.log('✅ Chat widget still functional');
}
" --environment="browser"
```

## 📋 **SYSTEMATIC DEBUGGING CHECKLIST**

### **After Each Major Change**
1. **Screenshot the full layout**: `./jtag interface/screenshot --querySelector="body" --filename="step-X-full.png"`
2. **Test existing chat widget**: Verify it still works
3. **Check console for errors**: Look at browser logs
4. **Test new functionality**: Use exec to interact with new components
5. **Validate CSS**: Check that styling is applied correctly

### **TEST-SAFE Development Iteration Cycle**
```bash
# 1. Make small changes to HTML/CSS/JS files
# 2. IMMEDIATELY validate tests
npm test                    # 🚨 MUST PASS - STOP IF FAILS
npx tsc --noEmit           # Compile check
# 3. ONLY if tests pass, proceed with visual validation
JTAG_WORKING_DIR="examples/widget-ui" npm start
# 4. Screenshot to see changes
./jtag interface/screenshot --querySelector=".desktop-container" --filename="iteration-$(date +%s).png"
# 5. Test interactions with exec
./jtag exec --code="/* test new functionality */" --environment="browser"
# 6. Check logs for any errors
# 7. If everything works, commit. If not, fix immediately.
# 8. Repeat with small increments
```

### **Problem Solving Process**
```bash
# When something doesn't work:
# 1. Take screenshot to see current state
./jtag interface/screenshot --querySelector="body" --filename="debug-current-state.png"

# 2. Check if elements exist
./jtag exec --code="
console.log('Debugging element existence...');
// Check specific selectors that should exist
" --environment="browser"

# 3. Check CSS computed styles
./jtag exec --code="
const element = document.querySelector('.problematic-element');
if (element) {
  const styles = getComputedStyle(element);
  console.log('Element styles:', {
    display: styles.display,
    position: styles.position,
    width: styles.width,
    height: styles.height
  });
}
" --environment="browser"

# 4. Make targeted fix
# 5. Screenshot to verify fix
./jtag interface/screenshot --querySelector=".fixed-element" --filename="debug-fixed.png"
```

## 🎯 **SPECIFIC DESKTOP LAYOUT DEBUGGING**

### **Phase 1: Grid Structure Debug**
```bash
# Verify basic grid is working
./jtag interface/screenshot --querySelector=".desktop-container" --filename="grid-structure.png"
./jtag exec --code="
const grid = document.querySelector('.desktop-container');
console.log('Grid computed style:', getComputedStyle(grid).gridTemplateColumns);
" --environment="browser"
```

### **Phase 2: Sidebar Components Debug**
```bash
# Test each sidebar component
./jtag interface/screenshot --querySelector="continuum-emoter" --filename="emoter-debug.png"
./jtag interface/screenshot --querySelector="dynamic-list" --filename="list-debug.png"
./jtag exec --code="
document.querySelector('[data-context=\"academy\"]').click();
console.log('Academy context clicked for testing');
" --environment="browser"
```

### **Phase 3: Draggable Separator Debug**
```bash
# Test separator functionality
./jtag exec --code="
const sep = document.querySelector('.draggable-separator');
const rect = sep.getBoundingClientRect();
console.log('Separator position:', rect);
console.log('Separator cursor style:', getComputedStyle(sep).cursor);
" --environment="browser"

# Simulate drag
./jtag exec --code="
const sep = document.querySelector('.draggable-separator');
sep.dispatchEvent(new MouseEvent('mousedown', { clientX: 250 }));
document.dispatchEvent(new MouseEvent('mousemove', { clientX: 300 }));
document.dispatchEvent(new MouseEvent('mouseup'));
" --environment="browser"

# Screenshot result
./jtag interface/screenshot --querySelector=".desktop-container" --filename="after-drag-test.png"
```

## 🚨 **MICRO-INCREMENT DEVELOPMENT STRATEGY**

### **Never Break npm test for More Than One Change**
```bash
# WRONG: Big changes that might break tests
# - Add entire desktop layout at once
# - Create multiple components simultaneously  
# - Make sweeping CSS changes

# RIGHT: Tiny incremental changes
# Step 1: Add just the desktop-container div
# Step 2: npm test (must pass)
# Step 3: Add basic CSS grid
# Step 4: npm test (must pass)
# Step 5: Add one sidebar component
# Step 6: npm test (must pass)
# ... continue with tiny steps
```

### **Rollback Strategy**
```bash
# If npm test fails after a change:
# 1. IMMEDIATELY identify what broke
npm test                    # See specific test failures
npx tsc --noEmit           # Check TypeScript errors

# 2. Either fix immediately or rollback
git stash                   # Rollback changes if fix isn't obvious
npm test                    # Verify tests pass again

# 3. Make smaller change and retry
# Don't proceed until npm test passes 100%
```

### **Safe Implementation Order**
```bash
# Phase 1: HTML structure only (no CSS, no JS)
# - Add desktop-container div
# - npm test ✅
# - Add sidebar divs  
# - npm test ✅

# Phase 2: Basic CSS (no JavaScript)
# - Add grid layout CSS
# - npm test ✅
# - Add sidebar styling
# - npm test ✅

# Phase 3: Components one by one
# - Add continuum-emoter component
# - npm test ✅
# - Add dynamic-list component  
# - npm test ✅

# Each step validated before proceeding
```

This ensures **npm test NEVER fails for long** and **nothing gets committed that breaks tests**!