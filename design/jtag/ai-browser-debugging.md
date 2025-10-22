# AI Browser Debugging with JTAG Console Probes

## 🛸 Real-Time Browser Inspection for Autonomous Development

The Continuum JTAG system enables AIs to debug browser state, inspect DOM, and execute JavaScript remotely through console probes. This creates a **live debugging bridge** between AI and browser environment.

## 🚀 Quick Start for AIs

### 1. Basic Probe Usage
```javascript
// Simple diagnostic probe
console.probe('🔍 Checking widget state', { widgets: discoveredWidgets });

// Advanced probe with JavaScript execution
console.probe({
  message: '🎨 DOM Investigation: Widget rendering status',
  executeJS: 'document.querySelectorAll("continuum-sidebar, chat-widget").length',
  category: 'widget-debug',
  tags: ['dom', 'widgets', 'rendering']
});
```

### 2. Monitor Probe Results
```bash
# Watch all AI diagnostics in real-time
tail -f .continuum/sessions/*/logs/browser.probe.json

# Human-readable probe logs
tail -f .continuum/sessions/*/logs/browser.log | grep "🛸 PROBE"

# Filter by category
grep '"category": "widget-debug"' browser.probe.json
```

## 🔬 Common AI Debugging Patterns

### Widget State Investigation
```javascript
console.probe({
  message: '🎨 Widget Registration Check',
  executeJS: `JSON.stringify({
    customElements: !!window.customElements,
    sidebarRegistered: typeof window.customElements?.get('continuum-sidebar'),
    chatRegistered: typeof window.customElements?.get('chat-widget'),
    domElements: {
      sidebar: !!document.querySelector('continuum-sidebar'),
      chat: !!document.querySelector('chat-widget')
    },
    innerHTML: {
      sidebar: document.querySelector('continuum-sidebar')?.innerHTML?.substring(0, 100),
      chat: document.querySelector('chat-widget')?.innerHTML?.substring(0, 100)
    }
  })`,
  category: 'widget-debug',
  tags: ['custom-elements', 'registration', 'dom']
});
```

### CSS and Styling Debug
```javascript
console.probe({
  message: '🎨 Style Analysis',
  executeJS: `JSON.stringify({
    stylesheets: document.styleSheets.length,
    computedStyles: {
      sidebar: getComputedStyle(document.querySelector('continuum-sidebar') || document.body).display,
      chat: getComputedStyle(document.querySelector('chat-widget') || document.body).display
    },
    cssRules: Array.from(document.styleSheets).map(sheet => {
      try { return sheet.cssRules?.length || 0; } catch(e) { return 'blocked'; }
    })
  })`,
  category: 'css-debug'
});
```

### API and Network Debug
```javascript
console.probe({
  message: '🌐 API Connection Status',
  executeJS: `JSON.stringify({
    continuumAPI: typeof window.continuum,
    websocketState: window.continuum?.state,
    sessionId: window.continuum?.sessionId,
    clientId: window.continuum?.clientId,
    methods: Object.keys(window.continuum || {}).filter(k => typeof window.continuum[k] === 'function')
  })`,
  category: 'api-debug'
});
```

### Performance and Resource Debug
```javascript
console.probe({
  message: '⚡ Performance Metrics',
  executeJS: `JSON.stringify({
    loadTime: performance.now(),
    memory: performance.memory ? {
      used: Math.round(performance.memory.usedJSHeapSize / 1024 / 1024),
      total: Math.round(performance.memory.totalJSHeapSize / 1024 / 1024)
    } : 'unavailable',
    resources: performance.getEntriesByType('resource').length,
    domNodes: document.querySelectorAll('*').length
  })`,
  category: 'performance'
});
```

## 🧠 AI Development Workflow

### 1. Hypothesis-Driven Debugging
```javascript
// Start with a hypothesis
console.probe('🔬 HYPOTHESIS: Widgets not rendering due to missing custom element registration');

// Test the hypothesis
console.probe({
  message: '🔍 Testing custom element registration',
  executeJS: 'Object.keys(window.customElements || {})',
  category: 'hypothesis-test'
});
```

### 2. Iterative Investigation
```javascript
// First probe: Broad investigation
console.probe({
  message: '🌐 Initial DOM Survey',
  executeJS: 'JSON.stringify({ title: document.title, bodyChildren: document.body.children.length })',
  category: 'survey'
});

// Second probe: Focus on findings
console.probe({
  message: '🔍 Widget Elements Deep Dive',
  executeJS: 'Array.from(document.querySelectorAll("*")).filter(el => el.tagName.includes("-")).map(el => ({ tag: el.tagName, innerHTML: el.innerHTML.substring(0, 50) }))',
  category: 'widget-focus'
});
```

### 3. Solution Validation
```javascript
// After implementing a fix, validate
console.probe({
  message: '✅ Validation: Widget system working',
  executeJS: `JSON.stringify({
    widgetCount: document.querySelectorAll('continuum-sidebar, chat-widget').length,
    hasContent: Array.from(document.querySelectorAll('continuum-sidebar, chat-widget')).map(el => el.innerHTML.length > 0),
    stylesApplied: Array.from(document.querySelectorAll('continuum-sidebar, chat-widget')).map(el => getComputedStyle(el).display !== 'none')
  })`,
  category: 'validation'
});
```

## 🎯 Advanced JTAG Techniques

### Automated Health Checks
```javascript
// Comprehensive system health probe
console.probe({
  message: '🏥 System Health Check',
  executeJS: `JSON.stringify({
    browser: {
      userAgent: navigator.userAgent.substring(0, 50),
      viewport: { width: window.innerWidth, height: window.innerHeight },
      online: navigator.onLine
    },
    continuum: {
      version: window.continuum?.version,
      state: window.continuum?.state,
      connected: window.continuum?.isConnected?.()
    },
    dom: {
      readyState: document.readyState,
      elements: document.querySelectorAll('*').length,
      customElements: document.querySelectorAll('*').filter(el => el.tagName.includes('-')).length
    },
    resources: {
      scripts: document.scripts.length,
      stylesheets: document.styleSheets.length,
      images: document.images.length
    }
  })`,
  category: 'health-check',
  tags: ['automated', 'comprehensive']
});
```

### Error Tracking
```javascript
// Capture and analyze errors
window.addEventListener('error', (error) => {
  console.probe({
    message: '❌ JavaScript Error Captured',
    data: {
      message: error.message,
      filename: error.filename,
      line: error.lineno,
      column: error.colno,
      stack: error.error?.stack?.substring(0, 500)
    },
    category: 'error-tracking'
  });
});
```

## 📊 Log Analysis Patterns

### Probe Log Structure
```json
{
  "message": "🔍 Human-readable description",
  "category": "debug-category",
  "tags": ["tag1", "tag2"],
  "data": { "structured": "data" },
  "executeJS": "actual_javascript_code",
  "timestamp": "2025-07-10T04:29:05.497Z",
  "sessionId": "development-shared-xyz"
}
```

### Filtering and Analysis
```bash
# Find specific issues
grep '"category": "widget-debug"' browser.probe.json | jq '.data'

# Track investigation timeline
grep "🔍\|🔬\|✅" browser.log | tail -20

# Extract JavaScript execution results
jq -r '.data.jsExecutionResult' browser.probe.json | jq '.'
```

## 🎪 AI Autonomous Development Flow

1. **🚀 Start**: `npm start` automatically opens browser and establishes probe connection
2. **🔍 Investigate**: Use probes to understand current state
3. **🛠️ Develop**: Make changes to code based on probe findings
4. **🔄 Validate**: Use probes to confirm fixes work
5. **📝 Document**: Probe logs serve as development documentation

## 🎯 Key Benefits for AIs

- **Real-time feedback**: See immediate results of code changes
- **No manual browser interaction**: Everything through console probes
- **Session correlation**: All debugging tied to specific development sessions
- **JavaScript execution**: Run any browser code for investigation
- **Structured logging**: Easy to parse and analyze programmatically
- **Category organization**: Filter probes by type of investigation

This JTAG system transforms browser debugging from a manual, visual process into an **automated, programmable interface** perfect for AI development workflows.