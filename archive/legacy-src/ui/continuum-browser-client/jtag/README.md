# JTAG Debugging System

Client-side reactive debugging system for automated widget monitoring and intelligent probing.

## 🛸 Features

- **Reactive Observers**: Automatic detection of widget state changes
- **Easy Probe API**: Simple, chainable methods for debugging
- **Event Triggers**: Automated responses to DOM mutations and lifecycle events
- **Performance Monitoring**: Memory, CPU, and resource tracking
- **Shadow DOM Analysis**: Deep inspection of custom element internals

## 🚀 Quick Start

```typescript
// Basic widget probe
jtag.widgets();

// Shadow DOM investigation
jtag.shadowDOM();

// Health check
jtag.health();

// Custom JavaScript execution
jtag.execute('document.querySelectorAll("continuum-sidebar").length');

// Watch for changes
const stopWatching = jtag.watch('widgets', 2000);

// Batch multiple probes
jtag.batch(['widgets', 'performance', 'network']);
```

## 🔍 Probe Methods

### Core Probes
- `jtag.widgets()` - Widget state analysis
- `jtag.shadowDOM()` - Shadow DOM content inspection
- `jtag.customElements()` - Custom element registration check
- `jtag.styles()` - CSS and styling analysis
- `jtag.performance()` - Performance metrics
- `jtag.network()` - Network and API status
- `jtag.health()` - Comprehensive health check

### Advanced Usage
- `jtag.execute(jsCode)` - Custom JavaScript execution
- `jtag.watch(method, interval)` - Reactive monitoring
- `jtag.batch(methods)` - Multiple probes at once

## 🔧 Observer System

The JTAG Observer automatically monitors:
- Widget registration and lifecycle events
- Shadow DOM content changes
- CSS injection and styling
- Performance degradation
- Error conditions

## 📊 Integration

The JTAG system integrates with:
- Console forwarding system
- Session-based logging
- Widget lifecycle events
- Browser DevTools
- Automated testing

## 🧪 Testing

Run module tests:
```bash
npm test
```

Run specific test types:
```bash
npm run test:unit
npm run test:integration
```

## 🎯 Use Cases

- **AI Debugging**: Automated widget state investigation
- **Development**: Real-time feedback during widget development
- **Testing**: Automated validation of widget functionality
- **Monitoring**: Production health checking
- **Troubleshooting**: Quick diagnosis of rendering issues