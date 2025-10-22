# AI Script Execution in Browser

## 🚀 Simplified JavaScript Execution for AIs

Beyond console probes, AIs can execute arbitrary JavaScript in the browser through the command system for more complex debugging and development tasks.

## 📝 Future Enhancement: Direct Script Execution Command

### Proposed `browser:execute` Command
```bash
# Simple expression evaluation
continuum browser:execute "document.title"

# Complex DOM manipulation
continuum browser:execute "document.querySelectorAll('continuum-sidebar').forEach(el => el.style.border = '2px solid red')"

# Multi-line script execution
continuum browser:execute --script="
  const widgets = document.querySelectorAll('continuum-sidebar, chat-widget');
  const report = {
    count: widgets.length,
    visible: Array.from(widgets).map(w => getComputedStyle(w).display !== 'none'),
    innerHTML: Array.from(widgets).map(w => w.innerHTML.length)
  };
  JSON.stringify(report, null, 2);
"
```

### Immediate Workaround: Use Console Probes
Until `browser:execute` is implemented, use console probes for script execution:

```javascript
// In browser console or through probe system
console.probe({
  message: '🔧 Script Execution',
  executeJS: `
    // Your JavaScript here
    const result = document.querySelectorAll('*').length;
    return { elementCount: result };
  `,
  category: 'script-execution'
});
```

## 🎯 AI Development Use Cases

### 1. Widget Debugging
```javascript
// Check widget registration and fix issues
console.probe({
  message: '🔧 Widget Registration Fix',
  executeJS: `
    // Attempt to register widgets if missing
    if (!window.customElements.get('continuum-sidebar')) {
      // This would normally require the widget class
      console.log('Widget classes not loaded - checking bundle');
    }
    
    // Return status
    JSON.stringify({
      sidebar: !!window.customElements.get('continuum-sidebar'),
      chat: !!window.customElements.get('chat-widget'),
      elementsInDOM: document.querySelectorAll('continuum-sidebar, chat-widget').length
    });
  `,
  category: 'widget-fix'
});
```

### 2. CSS Injection for Testing
```javascript
console.probe({
  message: '🎨 CSS Testing',
  executeJS: `
    // Inject test styles
    const style = document.createElement('style');
    style.textContent = \`
      continuum-sidebar { border: 2px solid blue !important; }
      chat-widget { border: 2px solid green !important; }
    \`;
    document.head.appendChild(style);
    
    // Return confirmation
    'CSS injected for widget visibility testing';
  `,
  category: 'css-testing'
});
```

### 3. API Testing
```javascript
console.probe({
  message: '🌐 API Testing',
  executeJS: `
    // Test Continuum API
    if (window.continuum) {
      return window.continuum.execute('health', {})
        .then(result => JSON.stringify({ apiWorking: true, healthResult: result }))
        .catch(error => JSON.stringify({ apiWorking: false, error: error.message }));
    } else {
      return JSON.stringify({ continuumAPI: false });
    }
  `,
  category: 'api-testing'
});
```

### 4. Real-time DOM Monitoring
```javascript
console.probe({
  message: '👁️ DOM Observer Setup',
  executeJS: `
    // Set up mutation observer for AI monitoring
    if (!window.aiDOMObserver) {
      const observer = new MutationObserver((mutations) => {
        console.probe({
          message: '🔄 DOM Changed',
          data: {
            mutations: mutations.length,
            types: mutations.map(m => m.type),
            targets: mutations.map(m => m.target.tagName).slice(0, 5)
          },
          category: 'dom-monitoring'
        });
      });
      
      observer.observe(document.body, {
        childList: true,
        subtree: true,
        attributes: true
      });
      
      window.aiDOMObserver = observer;
      return 'DOM observer started for AI monitoring';
    } else {
      return 'DOM observer already running';
    }
  `,
  category: 'monitoring-setup'
});
```

## 🛠️ Implementation Roadmap

### Phase 1: Enhanced Console Probes ✅
- [x] Console probe system with JS execution
- [x] Base64 encoding for safe transmission
- [x] Structured logging with categories
- [x] Real-time monitoring capabilities

### Phase 2: Direct Script Execution (Future)
- [ ] `browser:execute` command implementation
- [ ] Multi-line script support
- [ ] Return value handling
- [ ] Error capture and reporting
- [ ] Script timeout protection

### Phase 3: AI Development Tools (Future)
- [ ] Widget hot-reloading
- [ ] CSS live editing
- [ ] Component state inspection
- [ ] Performance profiling
- [ ] Automated testing framework

## 🎯 Current Capabilities

**What AIs Can Do Now:**
1. Execute any JavaScript in browser via console probes
2. Inspect DOM state and widget status
3. Test API connectivity and functionality
4. Monitor real-time changes
5. Inject CSS for testing
6. Set up event listeners and observers
7. Capture performance metrics
8. Debug widget registration issues

**Key Advantage:** All execution is logged and correlated with development sessions, creating a complete debugging trail for AI development workflows.

## 💡 AI Development Tips

1. **Start with Probes**: Use console.probe() for immediate JavaScript execution
2. **Category Organization**: Use consistent categories for easier log filtering
3. **Incremental Testing**: Break complex scripts into smaller, testable parts
4. **Error Handling**: Wrap risky operations in try-catch within executeJS
5. **State Preservation**: Use window globals to maintain state between probes
6. **Documentation**: Each probe serves as documentation of investigation process

This system enables **true autonomous browser development** where AIs can investigate, test, and validate changes without human intervention.