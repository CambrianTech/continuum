# JTAG Command - CLI Debugging Interface

Command-line interface for the JTAG browser debugging system. Provides easy CLI access to probe widget states, DOM content, and performance metrics in the running browser.

## 🚀 Quick Start

```bash
# Check widget states
./continuum jtag widgets

# Investigate shadow DOM
./continuum jtag shadowDOM

# System health check  
./continuum jtag health

# Execute custom JavaScript
./continuum jtag execute --code "document.title"

# Run multiple probes
./continuum jtag batch --methods widgets,health

# Watch for changes
./continuum jtag widgets --watch --interval 5000
```

## 🔍 Available Probes

### Core Probes
- **widgets** - Analyze widget registration, rendering, and shadow DOM state
- **shadowDOM** - Deep inspection of shadow DOM content and styling
- **customElements** - Check custom element registration and instances
- **performance** - Memory usage, timing metrics, and resource analysis
- **network** - API connectivity, WebSocket status, and latency
- **health** - Comprehensive system health with issue detection

### Screenshot Debugging
- **screenshot** - Take validation screenshots with red UUID indicators
- **validation** - Create git hook validation screenshots for visual debugging
- **interaction** - Capture before/after screenshots for UI interactions
- **chat** - Test chat widget interactions with visual validation

### Custom Execution
- **execute** - Run arbitrary JavaScript in browser context
- **batch** - Execute multiple probes simultaneously

## 📊 Output Formats

### Summary (Default)
Human-readable summary with key insights and recommendations:
```bash
./continuum jtag widgets --format summary
```

### JSON
Raw structured data for programmatic use:
```bash
./continuum jtag widgets --format json
```

### Table
Tabular format for easy scanning:
```bash
./continuum jtag widgets --format table
```

## 🔄 Watch Mode

Monitor changes in real-time:
```bash
# Watch widgets every 3 seconds (default)
./continuum jtag widgets --watch

# Custom interval
./continuum jtag widgets --watch --interval 10000

# Press Ctrl+C to stop watching
```

## 🎯 Advanced Usage

### Targeted Shadow DOM Analysis
```bash
# Analyze specific widget
./continuum jtag shadowDOM --selector "continuum-sidebar"

# Check all custom elements
./continuum jtag shadowDOM --selector "*"
```

### Batch Operations
```bash
# Quick health overview
./continuum jtag batch --methods widgets,health,performance

# Full system analysis
./continuum jtag batch --methods widgets,shadowDOM,customElements,performance,network,health
```

### Custom JavaScript Execution
```bash
# Simple property access
./continuum jtag execute --code "window.location.href"

# Complex DOM queries
./continuum jtag execute --code "document.querySelectorAll('continuum-sidebar')[0].shadowRoot.innerHTML.length"

# Widget method calls
./continuum jtag execute --code "document.querySelector('chat-widget').render()"
```

### Screenshot Debugging Commands
```bash
# Take validation screenshot with red UUID indicator
./continuum jtag screenshot --type validation

# Create git hook validation screenshot
./continuum jtag validation --hook pre-commit

# Test chat interaction with before/after screenshots
./continuum jtag chat --message "test message" --widget "chat-widget"

# Test widget interaction with visual validation
./continuum jtag interaction --selector "button" --action click

# Run complete git hook integration test
./continuum jtag validation --test-suite
```

## 📋 Log Integration

All probe results are automatically logged to:
- **Browser logs**: `.continuum/sessions/*/logs/browser.probe.json`
- **Browser console**: `http://localhost:9000` (DevTools)

Monitor in real-time:
```bash
# Watch all probe results
tail -f .continuum/sessions/*/logs/browser.probe.json

# Filter by category
grep '"category": "jtag-cli"' .continuum/sessions/*/logs/browser.probe.json
```

## 🛠️ Development Workflow

### 1. Quick Health Check
```bash
./continuum jtag health
```

### 2. Widget Debugging
```bash
# Check widget states
./continuum jtag widgets

# Investigate rendering issues
./continuum jtag shadowDOM

# Monitor changes during development
./continuum jtag widgets --watch
```

### 3. Performance Analysis
```bash
# Memory and timing analysis
./continuum jtag performance

# Network connectivity
./continuum jtag network
```

### 4. Custom Debugging
```bash
# Test specific functionality
./continuum jtag execute --code "typeof window.jtag"

# Batch analysis
./continuum jtag batch --methods widgets,performance
```

### 5. Visual Debugging & Validation
```bash
# Quick validation screenshot
./continuum jtag screenshot --type validation

# Git hook integration testing
./continuum jtag validation --test-suite

# Monitor chat interactions
./continuum jtag chat --message "test" --widget "chat-widget"

# Debug specific widget interactions
./continuum jtag interaction --selector "menu-button" --action click
```

## 🎯 Integration with Development Tools

The JTAG command integrates seamlessly with:
- **npm start** - Automatic browser connection
- **Session management** - Session-aware logging
- **Browser DevTools** - Real-time console integration
- **CI/CD pipelines** - JSON output for automation
- **Testing frameworks** - Automated widget validation

## 🔗 Cross-Platform Architecture

The command uses strongly typed interfaces from `/src/shared/types/JTAGSharedTypes.ts` ensuring compatibility across:
- CLI commands (this module)
- Browser client (`window.jtag`)
- Server daemons
- WebSocket communications
- API endpoints

This enables seamless debugging across the entire Continuum system.