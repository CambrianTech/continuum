# Continuum Agent Development Guide

Welcome to Continuum! This guide helps you get started with agent-driven development using our comprehensive toolkit.

## 🚀 Quick Start - Complete UI Development Workflow

**New developers should start here!** The [`fix_ui_styling_with_feedback.py`](../python-client/examples/fix_ui_styling_with_feedback.py) example demonstrates the complete development cycle:

```bash
cd python-client/examples
python fix_ui_styling_with_feedback.py
```

## 🧪 Agent Validation and Debugging

For debugging agent connections and validating the complete system, use:

```bash
cd /Users/joel/Development/ideem/vHSM/externals/continuum/python-client && source ../.continuum/venv/agents/bin/activate && python continuum_client.py Claude
```

This command validates:
- ✅ **Remote JavaScript execution** capability
- ✅ **Version reading** from browser UI (v0.2.1987)
- ✅ **Error/warning generation** in browser console
- ✅ **Screenshot capture** with full dark UI theme (187KB screenshots)
- ✅ **WebSocket communication** between Python agents and browser
- ✅ **File saving** to `.continuum/screenshots/` directory

**Screenshots automatically capture the complete dark cyberpunk UI** including sidebar, chat area, and all interface elements.

### What This Example Teaches

This comprehensive example demonstrates the **complete UI development workflow** that makes Continuum unique:

1. **📸 Visual Assessment** - Take "before" screenshots to understand current state
2. **🎨 Live CSS Injection** - Apply fixes immediately with JavaScript injection
3. **💬 Real-time Feedback** - Get detailed console output and debugging info
4. **⚡ Performance Testing** - Test search debouncing and UI responsiveness
5. **📊 Before/After Comparison** - Verify improvements with screenshot comparison
6. **🔄 Source Code Commits** - Apply working fixes to permanent source files
7. **✅ Persistence Verification** - Restart and verify fixes remain active

### Key Learning Points

- **See → Test → Fix → Verify** - The core development cycle
- **JavaScript feedback** shows you exactly what's happening in the browser
- **Screenshot workflows** provide visual proof of improvements
- **Live CSS testing** before committing to source code
- **Performance optimization** through search debouncing
- **Component styling consistency** across the UI

## 🛠️ Development Patterns

### Pattern 1: Screenshot-Driven Development
```python
# 1. Take before screenshot
before = await capture_screenshot(element)

# 2. Apply live fixes with JS
await apply_live_css_fixes()

# 3. Take after screenshot  
after = await capture_screenshot(element)

# 4. Compare and commit if successful
```

### Pattern 2: JavaScript Feedback Loop
```python
js_with_feedback = """
console.log('🔧 Starting fix...');
// Apply changes
console.log('✅ Fix applied successfully');
return JSON.stringify({success: true, details: {...}});
"""

result = await client.js.get_value(js_with_feedback)
```

### Pattern 3: Performance Testing
```python
# Test before optimization
performance_before = await test_performance()

# Apply optimizations
await apply_debouncing()

# Test after optimization
performance_after = await test_performance()

# Compare results
```

## 📚 Progressive Learning Path

### Beginner: Start with Screenshots
1. [`simple_screenshot.py`](../python-client/examples/simple_screenshot.py) - Basic capture
2. [`find_and_capture.py`](../python-client/examples/find_and_capture.py) - Element finding

### Intermediate: UI Development
3. [`fix_ui_styling_with_feedback.py`](../python-client/examples/fix_ui_styling_with_feedback.py) - Complete workflow ⭐

### Advanced: Custom Automation
4. [`natural_glass_submenu_demo.py`](../python-client/examples/natural_glass_submenu_demo.py) - Complex UI interactions
5. Build your own automation scripts using the patterns above

## 🔌 Connection and Architecture

### WebSocket Communication
Continuum uses WebSocket connections for real-time browser control:

```python
from continuum_client import ContinuumClient

async with ContinuumClient() as client:
    # Register as an agent
    await client.register_agent({
        'agentId': 'my-agent',
        'agentName': 'My Development Agent', 
        'agentType': 'ai'
    })
    
    # Execute JavaScript with feedback
    result = await client.js.get_value("""
        console.log('Hello from agent!');
        return 'Success';
    """)
```

### Promise Post Office System
Our unique architecture provides:
- ✅ **Promise-like JavaScript execution** from Python
- ✅ **Real-time console feedback** and error handling
- ✅ **Concurrent operation support** with proper routing
- ✅ **Screenshot capture** with multiple formats
- ✅ **Cross-platform compatibility** (macOS, Linux, Windows)

## 🎯 Best Practices

### 1. Always Use Feedback
```javascript
// Good - provides feedback
console.log('🔧 Starting operation...');
// do work
console.log('✅ Operation completed');

// Bad - silent operation
// do work with no feedback
```

### 2. Take Before/After Screenshots
```python
# Always document your changes visually
before_path = await take_screenshot('before')
await apply_fixes()
after_path = await take_screenshot('after')
```

### 3. Test Performance
```python
# Measure impact of your changes
before_metrics = await measure_performance()
await apply_optimization()
after_metrics = await measure_performance()
```

### 4. Use Descriptive Logging
```python
print("🎨 FIXING LEFT SIDEBAR STYLING")
print("=" * 40)
print("📸 Taking before screenshot...")
# Much better than just print("Starting...")
```

## 🔧 Troubleshooting

### Common Issues

**"Connection refused"** 
- Ensure Continuum server is running: `continuum`
- Check port configuration in `~/.continuum/config.env`

**"Screenshot failed"**
- Browser may not be connected
- html2canvas library may not be loaded
- Element may not be visible

**"JavaScript execution timeout"**
- Increase timeout in `client.js.get_value(js, timeout=30)`
- Check browser console for JavaScript errors

### Debug Steps
1. Check `continuum --help` for current status
2. Verify WebSocket connection with simple JS execution
3. Test screenshot capture with known elements
4. Use console.log extensively in your JavaScript
5. Check the `.continuum/screenshots/` directory for saved images

## 📖 Additional Resources

- [`python-client/examples/README.md`](../python-client/examples/README.md) - All examples explained
- [`python-client/README.md`](../python-client/README.md) - Client API documentation  
- [`agent-scripts/README.md`](../agent-scripts/README.md) - Agent automation tools
- [`docs/DEBUGGING_UTILITIES.md`](DEBUGGING_UTILITIES.md) - Advanced debugging techniques

## 🎉 Ready to Start?

Run the complete UI development workflow example to see everything in action:

```bash
cd python-client/examples
python fix_ui_styling_with_feedback.py
```

This will walk you through the entire process with real-time feedback, screenshots, and performance testing. Perfect for understanding how Continuum enables rapid, visual development workflows!

---

*Welcome to the future of agent-driven development with Continuum! 🚀*