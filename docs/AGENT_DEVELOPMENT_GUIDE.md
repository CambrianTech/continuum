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

## 🎨 Developing Widgets as an Agent (Like Claude)

When working as an AI agent like Claude, you can develop and test UI widgets using this iterative workflow:

### Step 1: Connect and Validate Your Agent Environment

```bash
cd /Users/joel/Development/ideem/vHSM/externals/continuum/python-client && source ../.continuum/venv/agents/bin/activate && python continuum_client.py Claude
```

This establishes your AgentClientConnection and validates:
- 🔗 **WebSocket communication** with the browser
- 📸 **Screenshot capability** for visual feedback  
- 🖥️ **JavaScript execution** for live testing
- 📋 **Console access** for debugging

### Step 2: Visual Assessment and Planning

Use the screenshot capability to understand the current UI state:

```javascript
// Take a screenshot to see current state
const takeScreenshot = () => {
    html2canvas(document.querySelector("body > div") || document.body, {
        allowTaint: true,
        useCORS: true,
        scale: 0.8,
        backgroundColor: "#0f1419"
    }).then(canvas => {
        const dataURL = canvas.toDataURL('image/png');
        const timestamp = Date.now();
        const filename = `widget-dev-${timestamp}.png`;
        
        window.ws.send(JSON.stringify({
            type: 'screenshot_data',
            filename: filename,
            dataURL: dataURL,
            timestamp: timestamp,
            source: 'widget_development',
            dimensions: { width: canvas.width, height: canvas.height }
        }));
    });
};
```

### Step 3: Live Widget Development with JavaScript Injection

Test widget components directly in the browser before committing to source files:

```javascript
// Example: Create a new widget dynamically
const createTestWidget = () => {
    const widget = document.createElement('div');
    widget.className = 'test-widget';
    widget.innerHTML = `
        <div style="
            background: linear-gradient(135deg, #0f1419 0%, #1a1f2e 100%);
            border: 1px solid #2a2f3e;
            border-radius: 8px;
            padding: 16px;
            margin: 8px;
            color: #e0e6ed;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
        ">
            <h3 style="margin: 0 0 8px 0; color: #00d4ff;">Test Widget</h3>
            <p style="margin: 0; opacity: 0.8;">This is a test widget for development</p>
        </div>
    `;
    
    // Insert into sidebar or main area
    const targetContainer = document.querySelector('.sidebar') || document.querySelector('.main-content');
    if (targetContainer) {
        targetContainer.appendChild(widget);
        console.log('✅ Test widget created successfully');
    }
};

// Test the widget
createTestWidget();
```

### Step 4: Iterative Design with CSS Hot-Reloading

Apply CSS changes instantly and test responsiveness:

```javascript
// Hot-reload CSS for rapid iteration
const applyCSSHotFix = (cssRules) => {
    const styleId = 'widget-dev-styles';
    let styleElement = document.getElementById(styleId);
    
    if (!styleElement) {
        styleElement = document.createElement('style');
        styleElement.id = styleId;
        document.head.appendChild(styleElement);
    }
    
    styleElement.textContent = cssRules;
    console.log('🎨 CSS hot-reloaded');
};

// Example: Test different widget styles
applyCSSHotFix(`
    .test-widget {
        transition: all 0.3s ease;
    }
    .test-widget:hover {
        transform: translateY(-2px);
        box-shadow: 0 4px 12px rgba(0, 212, 255, 0.2);
    }
`);
```

### Step 5: Component Testing and Interaction

Test widget functionality and user interactions:

```javascript
// Test widget interactions
const testWidgetInteractions = () => {
    const widget = document.querySelector('.test-widget');
    if (widget) {
        // Test hover effects
        widget.addEventListener('mouseenter', () => {
            console.log('🎯 Widget hover detected');
        });
        
        // Test click handling
        widget.addEventListener('click', () => {
            console.log('🖱️ Widget clicked');
            // Test state changes
            widget.style.borderColor = '#00d4ff';
        });
        
        console.log('✅ Widget interactions configured');
    }
};

testWidgetInteractions();
```

### Step 6: Screenshot Comparison and Validation

Capture before/after screenshots to validate improvements:

```javascript
// Take comparison screenshots
const compareWidgetStates = async () => {
    console.log('📸 Taking before screenshot...');
    takeScreenshot(); // Captures current state
    
    // Wait a moment, then apply changes
    setTimeout(() => {
        // Apply your widget changes here
        applyCSSHotFix(/* your updated CSS */);
        
        setTimeout(() => {
            console.log('📸 Taking after screenshot...');
            takeScreenshot(); // Captures improved state
        }, 500);
    }, 1000);
};
```

### Step 7: Commit Working Changes to Source Files

Once satisfied with the widget behavior, integrate into source files:

1. **Update component files** in `src/ui/components/`
2. **Update CSS styles** in `src/ui/styles/`
3. **Update UIGenerator** to include the new widget
4. **Test persistence** by refreshing and validating

### Step 8: Full System Validation

Run the complete validation to ensure your widget works in the full system:

```bash
cd /Users/joel/Development/ideem/vHSM/externals/continuum/python-client && source ../.continuum/venv/agents/bin/activate && python continuum_client.py Claude
```

This validates:
- ✅ Your widget renders correctly in screenshots
- ✅ JavaScript interactions work properly
- ✅ CSS styling is applied consistently
- ✅ No console errors are introduced

### Agent Development Best Practices

1. **Start with Screenshots** - Always capture the current state before making changes
2. **Iterate Rapidly** - Use JavaScript injection for fast testing cycles
3. **Test Interactions** - Verify hover, click, and state changes work correctly
4. **Validate Visually** - Use screenshot comparison to confirm improvements
5. **Commit Incrementally** - Move working changes to source files step by step
6. **Full Validation** - Run the agent validation command to ensure system integrity

This workflow allows you to develop widgets efficiently while maintaining the high quality and visual consistency of the Continuum interface.

## 🔍 Iterative Debugging: Console Errors and Visual Feedback

When developing as an agent, use this systematic approach to identify and fix issues through console monitoring and visual feedback:

### Step 1: Monitor Console Errors in Real-Time

Set up continuous console monitoring to catch errors as they occur:

```javascript
// Enhanced console monitoring for development
const setupConsoleMonitoring = () => {
    // Store original console methods
    const originalError = console.error;
    const originalWarn = console.warn;
    const originalLog = console.log;
    
    // Override console methods to capture errors
    console.error = function(...args) {
        originalError.apply(console, args);
        // Send error to server for agent monitoring
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            window.ws.send(JSON.stringify({
                type: 'console_log',
                level: 'error',
                message: args.join(' '),
                timestamp: Date.now(),
                source: 'widget_development'
            }));
        }
    };
    
    console.warn = function(...args) {
        originalWarn.apply(console, args);
        if (window.ws && window.ws.readyState === WebSocket.OPEN) {
            window.ws.send(JSON.stringify({
                type: 'console_log',
                level: 'warn',
                message: args.join(' '),
                timestamp: Date.now(),
                source: 'widget_development'
            }));
        }
    };
    
    // Monitor unhandled promise rejections
    window.addEventListener('unhandledrejection', (event) => {
        console.error('🚨 Unhandled Promise Rejection:', event.reason);
    });
    
    // Monitor general JavaScript errors
    window.addEventListener('error', (event) => {
        console.error('🚨 JavaScript Error:', event.message, 'at', event.filename + ':' + event.lineno);
    });
    
    console.log('🔍 Console monitoring enabled for widget development');
};

// Start monitoring
setupConsoleMonitoring();
```

### Step 2: Create Visual Error Indicators

Add visual feedback for errors directly in the UI:

```javascript
// Visual error indicator system
const createErrorIndicator = (message, type = 'error') => {
    const indicator = document.createElement('div');
    indicator.className = `error-indicator error-${type}`;
    indicator.style.cssText = `
        position: fixed;
        top: 20px;
        right: 20px;
        background: ${type === 'error' ? '#ff4444' : '#ffaa00'};
        color: white;
        padding: 12px 16px;
        border-radius: 8px;
        font-family: monospace;
        font-size: 12px;
        max-width: 300px;
        z-index: 10000;
        animation: slideIn 0.3s ease;
        box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    `;
    indicator.innerHTML = `
        <strong>${type.toUpperCase()}:</strong><br>
        ${message}
        <div style="margin-top: 8px; text-align: right;">
            <button onclick="this.parentElement.remove()" style="
                background: rgba(255,255,255,0.2);
                border: none;
                color: white;
                padding: 4px 8px;
                border-radius: 4px;
                cursor: pointer;
            ">×</button>
        </div>
    `;
    
    document.body.appendChild(indicator);
    
    // Auto-remove after 5 seconds
    setTimeout(() => {
        if (indicator.parentElement) {
            indicator.remove();
        }
    }, 5000);
};

// Add CSS animation
const style = document.createElement('style');
style.textContent = `
    @keyframes slideIn {
        from { transform: translateX(100%); opacity: 0; }
        to { transform: translateX(0); opacity: 1; }
    }
`;
document.head.appendChild(style);
```

### Step 3: Systematic Error Testing and Resolution

Use this workflow to identify and fix issues iteratively:

```javascript
// Comprehensive error testing workflow
const runWidgetErrorTests = () => {
    console.log('🧪 Starting widget error tests...');
    
    const tests = [
        {
            name: 'DOM Element Existence',
            test: () => {
                const requiredElements = ['.sidebar', '.main-content', '.version-badge'];
                const missing = requiredElements.filter(sel => !document.querySelector(sel));
                if (missing.length > 0) {
                    throw new Error(`Missing required elements: ${missing.join(', ')}`);
                }
                return '✅ All required DOM elements found';
            }
        },
        {
            name: 'WebSocket Connection',
            test: () => {
                if (!window.ws || window.ws.readyState !== WebSocket.OPEN) {
                    throw new Error('WebSocket not connected');
                }
                return '✅ WebSocket connection active';
            }
        },
        {
            name: 'CSS Dependencies',
            test: () => {
                const testElement = document.createElement('div');
                testElement.style.display = 'none';
                testElement.className = 'test-css-check';
                document.body.appendChild(testElement);
                
                const computedStyle = getComputedStyle(testElement);
                document.body.removeChild(testElement);
                
                if (!computedStyle) {
                    throw new Error('CSS computation failed');
                }
                return '✅ CSS system working';
            }
        },
        {
            name: 'Widget Creation Capability',
            test: () => {
                const testWidget = document.createElement('div');
                testWidget.innerHTML = '<span>Test</span>';
                
                const container = document.querySelector('.sidebar');
                if (!container) {
                    throw new Error('No container available for widgets');
                }
                
                container.appendChild(testWidget);
                container.removeChild(testWidget);
                return '✅ Widget creation capability confirmed';
            }
        }
    ];
    
    tests.forEach(test => {
        try {
            const result = test.test();
            console.log(`${test.name}: ${result}`);
        } catch (error) {
            console.error(`${test.name}: ❌ ${error.message}`);
            createErrorIndicator(`${test.name}: ${error.message}`, 'error');
        }
    });
    
    console.log('🧪 Widget error tests completed');
};

// Run tests
runWidgetErrorTests();
```

### Step 4: Visual Diff and Regression Testing

Compare screenshots to detect visual regressions:

```javascript
// Visual regression testing workflow
const performVisualRegression = async () => {
    console.log('📸 Starting visual regression test...');
    
    // Take baseline screenshot
    const takeBaselineScreenshot = () => {
        return new Promise((resolve) => {
            html2canvas(document.querySelector("body > div") || document.body, {
                allowTaint: true,
                useCORS: true,
                scale: 0.8,
                backgroundColor: "#0f1419"
            }).then(canvas => {
                const dataURL = canvas.toDataURL('image/png');
                const timestamp = Date.now();
                const filename = `baseline-${timestamp}.png`;
                
                window.ws.send(JSON.stringify({
                    type: 'screenshot_data',
                    filename: filename,
                    dataURL: dataURL,
                    timestamp: timestamp,
                    source: 'baseline_test',
                    dimensions: { width: canvas.width, height: canvas.height }
                }));
                
                console.log('📸 Baseline screenshot captured');
                resolve(filename);
            });
        });
    };
    
    // Take comparison screenshot after changes
    const takeComparisonScreenshot = () => {
        return new Promise((resolve) => {
            setTimeout(() => {
                html2canvas(document.querySelector("body > div") || document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.8,
                    backgroundColor: "#0f1419"
                }).then(canvas => {
                    const dataURL = canvas.toDataURL('image/png');
                    const timestamp = Date.now();
                    const filename = `comparison-${timestamp}.png`;
                    
                    window.ws.send(JSON.stringify({
                        type: 'screenshot_data',
                        filename: filename,
                        dataURL: dataURL,
                        timestamp: timestamp,
                        source: 'comparison_test',
                        dimensions: { width: canvas.width, height: canvas.height }
                    }));
                    
                    console.log('📸 Comparison screenshot captured');
                    resolve(filename);
                });
            }, 1000);
        });
    };
    
    // Execute regression test
    const baseline = await takeBaselineScreenshot();
    
    // Apply your widget changes here
    console.log('🔄 Apply your widget changes now...');
    
    const comparison = await takeComparisonScreenshot();
    
    console.log(`📊 Visual regression test complete:
        Baseline: ${baseline}
        Comparison: ${comparison}
        Check .continuum/screenshots/ for visual diff`);
};

// Run visual regression test
performVisualRegression();
```

### Step 5: Automated Error Recovery

Implement self-healing for common issues:

```javascript
// Automated error recovery system
const setupErrorRecovery = () => {
    const recoveryActions = {
        'WebSocket disconnected': () => {
            console.log('🔄 Attempting WebSocket reconnection...');
            // Reconnection logic would go here
            setTimeout(() => {
                if (window.ws && window.ws.readyState !== WebSocket.OPEN) {
                    location.reload();
                }
            }, 5000);
        },
        
        'Widget render failed': () => {
            console.log('🔄 Clearing widget cache and retrying...');
            const widgets = document.querySelectorAll('.test-widget');
            widgets.forEach(widget => widget.remove());
            
            setTimeout(() => {
                createTestWidget(); // Retry widget creation
            }, 1000);
        },
        
        'CSS not applied': () => {
            console.log('🔄 Reapplying CSS styles...');
            const existingStyles = document.getElementById('widget-dev-styles');
            if (existingStyles) {
                existingStyles.remove();
            }
            // Reapply CSS
            applyCSSHotFix(/* your CSS rules */);
        }
    };
    
    // Monitor for specific error patterns
    const originalError = console.error;
    console.error = function(...args) {
        originalError.apply(console, args);
        
        const errorMessage = args.join(' ');
        Object.keys(recoveryActions).forEach(pattern => {
            if (errorMessage.includes(pattern)) {
                console.log(`🚑 Auto-recovery triggered for: ${pattern}`);
                recoveryActions[pattern]();
            }
        });
    };
    
    console.log('🚑 Error recovery system enabled');
};

setupErrorRecovery();
```

### Best Practices for Iterative Debugging

1. **Monitor Continuously** - Set up console monitoring before making any changes
2. **Visual Feedback First** - Use error indicators to immediately see issues
3. **Test Systematically** - Run comprehensive tests for each change
4. **Screenshot Everything** - Capture visual state before and after changes
5. **Automate Recovery** - Implement self-healing for common issues
6. **Log Comprehensively** - Send all debugging info back to your agent environment

This approach ensures you can quickly identify, diagnose, and fix issues while maintaining visual quality and system stability.

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