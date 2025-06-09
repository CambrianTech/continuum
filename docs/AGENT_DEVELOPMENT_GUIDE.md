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

## 🚌 Bus Command Architecture: How Screenshots Actually Work

The screenshot system uses Continuum's **bus command architecture**, not direct JavaScript injection. Here's the complete flow:

### Bus Command Flow

```mermaid
Python Agent → WebSocket → Server → Command Processor → Browser → Screenshot Service → File System
```

### Step 1: Python Agent Sends Bus Command

```python
# continuum_client.py sends this bus command
screenshot_cmd = {
    "type": "task",
    "role": "system", 
    "task": "[CMD:SCREENSHOT]"
}
await self.send_message(screenshot_cmd)
```

### Step 2: Server Routes to Command Processor

The WebSocket server receives the bus command and routes it to the appropriate command handler:

```javascript
// WebSocketServer.cjs processes the task
if (data.type === 'task') {
    const { task } = data;
    // Task contains "[CMD:SCREENSHOT]"
    // Route to CommandProcessor
}
```

### Step 3: ScreenshotCommand.cjs Executes

Located at `src/commands/core/ScreenshotCommand.cjs`, this command:

```javascript
static async execute(params, continuum) {
    // 1. Parse parameters (selector, coordinates, format, quality)
    const options = ScreenshotCommand.parseParams(params);
    
    // 2. Generate JavaScript for browser execution
    const jsCommand = ScreenshotCommand.generateScreenshotJS(options);
    
    // 3. Broadcast to browser via WebSocket
    continuum.webSocketServer.broadcast({
        type: 'execute_js',
        data: { command: jsCommand }
    });
    
    // 4. Wait for screenshot data to be received
    // 5. Save via ScreenshotService
}
```

### Step 4: Browser Executes Generated JavaScript

The command generates JavaScript that:

```javascript
function captureScreenshot() {
    const timestamp = Date.now();
    let filename = 'continuum-screenshot-' + timestamp + '.png';
    
    // Use our working configuration
    let targetElement = document.querySelector("body > div") || document.body;
    let captureOptions = {
        allowTaint: true,
        useCORS: true,
        scale: 0.8,
        backgroundColor: '#0f1419'  // Dark theme
    };
    
    html2canvas(targetElement, captureOptions).then(canvas => {
        const dataURL = canvas.toDataURL('image/png');
        
        // Send back to server via WebSocket
        window.ws.send(JSON.stringify({
            type: 'screenshot_data',
            filename: filename,
            dataURL: dataURL,
            timestamp: timestamp,
            dimensions: { width: canvas.width, height: canvas.height }
        }));
    });
}
```

### Step 5: Server Saves Screenshot

The WebSocketServer receives the screenshot data and saves it:

```javascript
// WebSocketServer.cjs handles screenshot_data
else if (data.type === 'screenshot_data') {
    const { dataURL, filename } = data;
    
    // Save to .continuum/screenshots directory
    const base64Data = dataURL.replace(/^data:image\/png;base64,/, '');
    const buffer = Buffer.from(base64Data, 'base64');
    const outputPath = path.join(process.cwd(), '.continuum', 'screenshots', filename);
    fs.writeFileSync(outputPath, buffer);
}
```

### Command Parameters

The SCREENSHOT command supports various parameters:

```javascript
// Basic screenshot
"[CMD:SCREENSHOT]"

// With selector
"[CMD:SCREENSHOT] {\"selector\": \".sidebar\"}"

// With coordinates  
"[CMD:SCREENSHOT] {\"x\": 100, \"y\": 200, \"width\": 800, \"height\": 600}"

// With format and quality
"[CMD:SCREENSHOT] {\"format\": \"jpeg\", \"quality\": 0.8}"
```

### Why Bus Commands vs Direct JavaScript

**Bus Commands provide:**
- ✅ **Standardized interface** - Consistent command structure
- ✅ **Parameter validation** - Built-in parsing and validation
- ✅ **Error handling** - Centralized error management
- ✅ **Logging and monitoring** - Complete command execution tracking
- ✅ **Reusability** - Commands can be called from any agent or interface
- ✅ **Security** - Controlled execution environment

**Direct JavaScript injection would be:**
- ❌ **Ad-hoc** - No standardization
- ❌ **Unsafe** - Direct code execution
- ❌ **Difficult to monitor** - No centralized logging
- ❌ **Hard to maintain** - Scattered code

### Testing the Bus Command System

Validate the complete bus command flow:

```bash
cd /Users/joel/Development/ideem/vHSM/externals/continuum/python-client && source ../.continuum/venv/agents/bin/activate && python continuum_client.py Claude
```

This executes:
1. **AgentClientConnection** established
2. **Bus command sent**: `[CMD:SCREENSHOT]`
3. **ScreenshotCommand.cjs** executes
4. **Browser captures** full dark UI (187KB screenshots)
5. **File saved** to `.continuum/screenshots/validation_screenshot_*.png`

The bus command architecture makes screenshot capture a **first-class system capability** rather than an ad-hoc script injection.

## 🐛 Debugging Client-Side JavaScript from Python Client

The Python client provides powerful debugging capabilities to capture browser console logs, errors, and JavaScript execution results in real-time.

### Method 1: Real-Time Console Monitoring

Create a comprehensive debugging session that captures all browser activity:

```python
import asyncio
import websockets
import json
import base64

class BrowserDebugger:
    def __init__(self, ws_url="ws://localhost:9000"):
        self.ws_url = ws_url
        self.ws = None
        
    async def connect(self):
        """Connect to Continuum WebSocket"""
        self.ws = await websockets.connect(self.ws_url)
        
        # Register as debugging agent
        register_msg = {
            "type": "agent_register",
            "agentName": "BrowserDebugger",
            "capabilities": ["console_monitoring", "error_capture", "js_execution"]
        }
        await self.ws.send(json.dumps(register_msg))
        print("🔗 Connected as Browser Debugger")
        
    async def enable_console_monitoring(self):
        """Enable real-time console monitoring in browser"""
        monitor_script = '''
        console.log('🔍 ENABLING BROWSER DEBUG MONITORING...');
        
        // Store original console methods
        const originalConsole = {
            log: console.log.bind(console),
            warn: console.warn.bind(console),
            error: console.error.bind(console),
            info: console.info.bind(console)
        };
        
        // Override console methods to capture and send to Python
        const sendToDebugger = (level, args) => {
            const message = args.map(arg => 
                typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
            ).join(' ');
            
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify({
                    type: 'debug_console',
                    level: level,
                    message: message,
                    timestamp: Date.now(),
                    url: window.location.href,
                    source: 'browser_debug_monitor'
                }));
            }
            
            // Call original console method
            originalConsole[level](args);
        };
        
        console.log = (...args) => sendToDebugger('log', args);
        console.warn = (...args) => sendToDebugger('warn', args);
        console.error = (...args) => sendToDebugger('error', args);
        console.info = (...args) => sendToDebugger('info', args);
        
        // Monitor JavaScript errors
        window.addEventListener('error', (event) => {
            const errorData = {
                type: 'debug_error',
                message: event.message,
                filename: event.filename,
                lineno: event.lineno,
                colno: event.colno,
                error: event.error ? event.error.toString() : null,
                stack: event.error ? event.error.stack : null,
                timestamp: Date.now()
            };
            
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify(errorData));
            }
        });
        
        // Monitor promise rejections
        window.addEventListener('unhandledrejection', (event) => {
            const rejectionData = {
                type: 'debug_rejection',
                reason: event.reason ? event.reason.toString() : 'Unknown',
                timestamp: Date.now()
            };
            
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify(rejectionData));
            }
        });
        
        console.log('✅ Browser debug monitoring enabled');
        return 'DEBUG_MONITORING_ENABLED';
        '''
        
        await self.send_js_command(monitor_script)
        print("✅ Console monitoring enabled")
        
    async def send_js_command(self, js_code):
        """Send JavaScript command to browser and wait for result"""
        encoded_js = base64.b64encode(js_code.encode()).decode()
        command = {
            "type": "task",
            "role": "system",
            "task": f"[CMD:BROWSER_JS] {encoded_js}"
        }
        
        await self.ws.send(json.dumps(command))
        
        # Wait for js_executed response
        return await self.wait_for_js_result()
        
    async def wait_for_js_result(self, timeout=10):
        """Wait for JavaScript execution result"""
        start_time = asyncio.get_event_loop().time()
        
        while asyncio.get_event_loop().time() - start_time < timeout:
            try:
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                if data.get("type") == "js_executed":
                    return {
                        "success": data.get("success", False),
                        "result": data.get("result"),
                        "output": data.get("output", []),
                        "error": data.get("error"),
                        "timestamp": data.get("timestamp")
                    }
                    
            except asyncio.TimeoutError:
                continue
                
        return {"success": False, "error": "Timeout waiting for result"}
        
    async def listen_for_debug_messages(self, duration=30):
        """Listen for debug messages from browser"""
        print(f"👂 Listening for debug messages for {duration} seconds...")
        start_time = asyncio.get_event_loop().time()
        
        while asyncio.get_event_loop().time() - start_time < duration:
            try:
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                # Handle different types of debug messages
                msg_type = data.get("type")
                
                if msg_type == "debug_console":
                    level = data.get("level", "log").upper()
                    message_text = data.get("message", "")
                    timestamp = data.get("timestamp", 0)
                    print(f"📱 [{level}] {message_text}")
                    
                elif msg_type == "debug_error":
                    print(f"🚨 JavaScript Error: {data.get('message')}")
                    print(f"   📁 File: {data.get('filename')}:{data.get('lineno')}")
                    if data.get('stack'):
                        print(f"   📚 Stack: {data.get('stack')[:200]}...")
                        
                elif msg_type == "debug_rejection":
                    print(f"💥 Promise Rejection: {data.get('reason')}")
                    
                elif msg_type == "screenshot_data":
                    filename = data.get("filename", "unknown")
                    dimensions = data.get("dimensions", {})
                    print(f"📸 Screenshot captured: {filename} ({dimensions.get('width')}x{dimensions.get('height')})")
                    
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                print(f"❌ Error listening: {e}")
                break
                
        print("👂 Debug listening session complete")

# Usage example
async def debug_browser_session():
    debugger = BrowserDebugger()
    await debugger.connect()
    await debugger.enable_console_monitoring()
    
    # Test some JavaScript
    test_result = await debugger.send_js_command('''
        console.log('🧪 Testing JavaScript execution');
        console.warn('⚠️ This is a test warning');
        console.error('🔴 This is a test error');
        
        // Test DOM manipulation
        const testDiv = document.createElement('div');
        testDiv.textContent = 'Debug test element';
        document.body.appendChild(testDiv);
        console.log('✅ DOM manipulation successful');
        
        // Test intentional error
        try {
            nonExistentFunction();
        } catch (e) {
            console.error('🚨 Caught intentional error:', e.message);
        }
        
        return 'DEBUG_TEST_COMPLETE';
    ''')
    
    print(f"📊 Test result: {test_result}")
    
    # Listen for real-time debug messages
    await debugger.listen_for_debug_messages(30)

# Run the debugger
if __name__ == "__main__":
    asyncio.run(debug_browser_session())
```

### Method 2: Quick Debug Commands

For quick debugging sessions, use these one-liner commands:

```bash
# Debug JavaScript execution with console output
cd /Users/joel/Development/ideem/vHSM/externals/continuum/python-client && source ../.continuum/venv/agents/bin/activate && python -c "
import asyncio
from continuum_client import ContinuumPythonClient

async def quick_debug():
    client = ContinuumPythonClient()
    await client.connect()
    
    result = await client.execute_browser_script('
        console.log(\"🔍 Quick debug test\");
        console.log(\"📊 DOM ready state:\", document.readyState);
        console.log(\"🌐 Current URL:\", window.location.href);
        console.log(\"📏 Viewport:\", window.innerWidth + \"x\" + window.innerHeight);
        return \"DEBUG_COMPLETE\";
    ')
    
    print(f\"Result: {result['success']}\")
    if result['output']:
        for entry in result['output']:
            print(f\"[{entry['level']}] {entry['message']}\")

asyncio.run(quick_debug())
"
```

### Method 3: Error Capture and Analysis

Capture and analyze JavaScript errors systematically:

```python
async def capture_browser_errors():
    debugger = BrowserDebugger()
    await debugger.connect()
    
    # Set up comprehensive error capture
    error_capture_script = '''
    console.log('🔍 Setting up comprehensive error capture...');
    
    const errorLog = [];
    
    // Capture syntax errors
    window.addEventListener('error', (e) => {
        const error = {
            type: 'syntax_error',
            message: e.message,
            filename: e.filename,
            line: e.lineno,
            column: e.colno,
            stack: e.error ? e.error.stack : null,
            timestamp: Date.now()
        };
        errorLog.push(error);
        console.error('🚨 Syntax Error Captured:', error);
    });
    
    // Capture promise rejections
    window.addEventListener('unhandledrejection', (e) => {
        const error = {
            type: 'promise_rejection',
            reason: e.reason ? e.reason.toString() : 'Unknown',
            timestamp: Date.now()
        };
        errorLog.push(error);
        console.error('💥 Promise Rejection Captured:', error);
    });
    
    // Function to get all captured errors
    window.getBrowserErrors = () => {
        console.log('📊 Retrieved', errorLog.length, 'errors');
        return errorLog;
    };
    
    console.log('✅ Error capture system ready');
    return 'ERROR_CAPTURE_ENABLED';
    '''
    
    result = await debugger.send_js_command(error_capture_script)
    print(f"Error capture setup: {result}")
    
    # Trigger some test errors
    test_errors_script = '''
    console.log('🧪 Testing error capture...');
    
    // Test 1: Reference error
    try {
        undefinedVariable.someMethod();
    } catch (e) {
        console.error('Test Error 1:', e.message);
    }
    
    // Test 2: Promise rejection
    Promise.reject('Test rejection reason');
    
    // Test 3: Async error
    setTimeout(() => {
        throw new Error('Test async error');
    }, 100);
    
    return 'ERROR_TESTS_TRIGGERED';
    '''
    
    await debugger.send_js_command(test_errors_script)
    
    # Wait for errors to be captured
    await asyncio.sleep(2)
    
    # Retrieve captured errors
    get_errors_result = await debugger.send_js_command('return window.getBrowserErrors();')
    
    if get_errors_result['success'] and get_errors_result['result']:
        errors = json.loads(get_errors_result['result'])
        print(f"📊 Captured {len(errors)} errors:")
        for i, error in enumerate(errors):
            print(f"  {i+1}. [{error['type']}] {error['message']}")
```

### Method 4: Live Widget Debugging

Debug widget development in real-time:

```python
async def debug_widget_development():
    debugger = BrowserDebugger()
    await debugger.connect()
    await debugger.enable_console_monitoring()
    
    # Create a test widget with debugging
    widget_script = '''
    console.log('🎨 Creating test widget with debugging...');
    
    const widget = document.createElement('div');
    widget.id = 'debug-test-widget';
    widget.innerHTML = `
        <div style="
            background: linear-gradient(135deg, #0f1419 0%, #1a1f2e 100%);
            border: 1px solid #00d4ff;
            border-radius: 8px;
            padding: 16px;
            margin: 16px;
            color: #e0e6ed;
            position: fixed;
            top: 20px;
            right: 20px;
            z-index: 10000;
            max-width: 300px;
        ">
            <h3 style="margin: 0 0 8px 0; color: #00d4ff;">Debug Widget</h3>
            <div id="debug-info">Initializing...</div>
            <button id="debug-test-btn" style="
                background: #00d4ff;
                color: #0f1419;
                border: none;
                padding: 4px 8px;
                border-radius: 4px;
                margin-top: 8px;
                cursor: pointer;
            ">Test Action</button>
        </div>
    `;
    
    document.body.appendChild(widget);
    console.log('✅ Debug widget created');
    
    // Add interaction logging
    const testBtn = document.getElementById('debug-test-btn');
    const debugInfo = document.getElementById('debug-info');
    
    testBtn.addEventListener('click', () => {
        console.log('🖱️ Debug button clicked');
        debugInfo.textContent = `Clicked at ${new Date().toLocaleTimeString()}`;
    });
    
    // Update debug info periodically
    setInterval(() => {
        debugInfo.textContent = `Active - ${new Date().toLocaleTimeString()}`;
        console.log('🔄 Debug widget heartbeat');
    }, 5000);
    
    return 'DEBUG_WIDGET_CREATED';
    '''
    
    result = await debugger.send_js_command(widget_script)
    print(f"Widget creation: {result}")
    
    # Listen for widget interactions
    print("🎯 Debug widget created. Interact with it and watch the console...")
    await debugger.listen_for_debug_messages(60)

# Usage
asyncio.run(debug_widget_development())
```

### Best Practices for Browser Debugging

1. **Start with Monitoring** - Always enable console monitoring first
2. **Capture Everything** - Log JavaScript errors, promise rejections, and console output
3. **Use Visual Feedback** - Create debug widgets for visual confirmation
4. **Test Systematically** - Use structured error testing
5. **Real-time Analysis** - Monitor browser activity as it happens
6. **Screenshot Validation** - Combine debugging with screenshot capture

This debugging approach gives you complete visibility into browser-side JavaScript execution, errors, and console output directly from your Python agent environment.

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