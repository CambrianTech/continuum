#!/usr/bin/env python3
"""
Client Debug Workflow - Systematic browser console debugging
This is the standard way to debug the Continuum client interface
"""

import asyncio
import websockets
import json
import base64
import time
from datetime import datetime

class ClientDebugger:
    def __init__(self, ws_url="ws://localhost:9000"):
        self.ws_url = ws_url
        
    async def capture_console_errors(self):
        """Capture all console errors with line numbers and stack traces"""
        try:
            async with websockets.connect(self.ws_url) as websocket:
                # JavaScript to capture detailed error information
                js_code = '''
                (function() {
                    console.log("=== CLIENT DEBUG SESSION STARTED ===");
                    
                    // Capture all console errors with stack traces
                    const originalError = console.error;
                    const errors = [];
                    
                    console.error = function(...args) {
                        const error = {
                            message: args.join(" "),
                            stack: new Error().stack,
                            timestamp: new Date().toISOString()
                        };
                        errors.push(error);
                        originalError.apply(console, args);
                    };
                    
                    // Check for script loading errors
                    window.addEventListener("error", function(e) {
                        console.error(`Script Error: ${e.message} at line ${e.lineno} in ${e.filename}`);
                    });
                    
                    // Component status check
                    console.log("=== COMPONENT STATUS ===");
                    const simpleSelector = document.querySelector("simple-agent-selector");
                    console.log("SimpleAgentSelector element found:", !!simpleSelector);
                    console.log("SimpleAgentSelector registered:", !!customElements.get("simple-agent-selector"));
                    
                    if (simpleSelector) {
                        console.log("Element tag:", simpleSelector.tagName);
                        console.log("Element shadowRoot:", !!simpleSelector.shadowRoot);
                        console.log("Element innerHTML length:", simpleSelector.innerHTML.length);
                    }
                    
                    // Script loading status
                    console.log("=== SCRIPT STATUS ===");
                    const componentScripts = Array.from(document.scripts)
                        .filter(s => s.src.includes("components"))
                        .map(s => ({
                            src: s.src.split("/").pop(),
                            loaded: s.readyState === "complete" || s.readyState === "",
                            error: s.onerror ? "Has error handler" : "No error handler"
                        }));
                    console.log("Component scripts:", componentScripts);
                    
                    // Return summary
                    return JSON.stringify({
                        simpleSelector: !!simpleSelector,
                        registered: !!customElements.get("simple-agent-selector"),
                        scripts: componentScripts,
                        errors: errors.slice(-5)
                    });
                })();
                '''
                
                encoded = base64.b64encode(js_code.encode()).decode()
                
                task_message = {
                    'type': 'task',
                    'role': 'system', 
                    'task': f'[CMD:BROWSER_JS] {encoded}'
                }
                await websocket.send(json.dumps(task_message))
                
                response = await websocket.recv()
                result = json.loads(response)
                
                return result.get('message', 'No response')
                
        except Exception as e:
            return f'WebSocket Error: {e}'
    
    async def check_component_loading(self, component_name):
        """Check specific component loading status"""
        try:
            async with websockets.connect(self.ws_url) as websocket:
                js_code = f'''
                console.log("=== {component_name.upper()} DEBUG ===");
                const element = document.querySelector("{component_name}");
                const registered = customElements.get("{component_name}");
                
                console.log("Element in DOM:", !!element);
                console.log("Custom element registered:", !!registered);
                
                if (element && !registered) {{
                    console.error("ERROR: Element exists but custom element not registered!");
                }}
                
                if (!element && registered) {{
                    console.error("ERROR: Custom element registered but not in DOM!");
                }}
                
                // Check script loading
                const scriptSrc = "/src/ui/components/" + "{component_name}".split("-").map(p => p.charAt(0).toUpperCase() + p.slice(1)).join("") + ".js";
                const script = Array.from(document.scripts).find(s => s.src.includes(scriptSrc));
                console.log("Script loaded:", !!script);
                if (script) {{
                    console.log("Script src:", script.src);
                    console.log("Script ready state:", script.readyState);
                }}
                
                "{component_name} debug complete";
                '''
                
                encoded = base64.b64encode(js_code.encode()).decode()
                
                task_message = {
                    'type': 'task',
                    'role': 'system', 
                    'task': f'[CMD:BROWSER_JS] {encoded}'
                }
                await websocket.send(json.dumps(task_message))
                
                response = await websocket.recv()
                result = json.loads(response)
                
                return result.get('message', 'No response')
                
        except Exception as e:
            return f'WebSocket Error: {e}'
    
    async def force_component_reload(self, component_name):
        """Force reload a component script"""
        try:
            async with websockets.connect(self.ws_url) as websocket:
                js_code = f'''
                console.log("=== FORCE RELOAD {component_name.upper()} ===");
                
                // Remove existing script
                const existingScript = Array.from(document.scripts)
                    .find(s => s.src.includes("{component_name}"));
                if (existingScript) {{
                    existingScript.remove();
                    console.log("Removed existing script");
                }}
                
                // Create new script element
                const script = document.createElement("script");
                script.src = "/src/ui/components/" + "{component_name}".split("-").map(p => p.charAt(0).toUpperCase() + p.slice(1)).join("") + ".js?t=" + Date.now();
                script.onload = () => console.log("Script reloaded successfully");
                script.onerror = (e) => console.error("Script reload failed:", e);
                document.head.appendChild(script);
                
                "Force reload initiated";
                '''
                
                encoded = base64.b64encode(js_code.encode()).decode()
                
                task_message = {
                    'type': 'task',
                    'role': 'system', 
                    'task': f'[CMD:BROWSER_JS] {encoded}'
                }
                await websocket.send(json.dumps(task_message))
                
                response = await websocket.recv()
                result = json.loads(response)
                
                return result.get('message', 'No response')
                
        except Exception as e:
            return f'WebSocket Error: {e}'

    async def take_screenshot_with_validation(self):
        """Take screenshot and validate basic UI elements are present"""
        try:
            async with websockets.connect(self.ws_url) as websocket:
                # Take screenshot
                task_message = {
                    'type': 'task',
                    'role': 'system', 
                    'task': '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
                }
                await websocket.send(json.dumps(task_message))
                
                response = await websocket.recv()
                result = json.loads(response)
                
                if 'message' in result and 'screenshot saved' in result['message']:
                    screenshot_path = result['message'].split('screenshot saved: ')[1]
                    return f"Screenshot saved: {screenshot_path}"
                else:
                    return f"Screenshot result: {result}"
                    
        except Exception as e:
            return f'Screenshot Error: {e}'
    
    async def check_version_and_connection(self):
        """Check server version and connection status"""
        try:
            async with websockets.connect(self.ws_url) as websocket:
                js_code = '''
                console.log("=== VERSION & CONNECTION CHECK ===");
                console.log("Client version:", window.CLIENT_VERSION || "Unknown");
                console.log("Server version:", window.SERVER_VERSION || "Unknown");
                console.log("WebSocket ready state:", window.ws ? window.ws.readyState : "No WebSocket");
                console.log("Document ready state:", document.readyState);
                console.log("Current URL:", window.location.href);
                console.log("User agent:", navigator.userAgent);
                "Version check complete";
                '''
                
                encoded = base64.b64encode(js_code.encode()).decode()
                
                task_message = {
                    'type': 'task',
                    'role': 'system', 
                    'task': f'[CMD:BROWSER_JS] {encoded}'
                }
                await websocket.send(json.dumps(task_message))
                
                response = await websocket.recv()
                result = json.loads(response)
                
                return result.get('message', 'No response')
                
        except Exception as e:
            return f'Version Check Error: {e}'

async def main():
    """Run complete client debugging session with QA validation"""
    debugger = ClientDebugger()
    
    print("🔍 CLIENT DEBUG WORKFLOW - QA VALIDATION")
    print("=" * 60)
    print(f"🕐 Started at: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    
    print("\n🔌 STEP 1: Connection & Version Validation")
    print("-" * 40)
    version_status = await debugger.check_version_and_connection()
    print(f"Version Status: {version_status}")
    
    if "Error" in str(version_status):
        print("❌ CRITICAL: WebSocket connection failed!")
        print("🔧 Server may be down or port misconfigured")
        return
    
    print("\n📸 STEP 2: Initial Screenshot Validation")
    print("-" * 40)
    screenshot_result = await debugger.take_screenshot_with_validation()
    print(f"Screenshot: {screenshot_result}")
    
    if "Error" in str(screenshot_result):
        print("❌ WARNING: Screenshot capture failed!")
        print("🔧 Browser may not be responding to commands")
    
    print("\n🔍 STEP 3: Console Error Analysis")
    print("-" * 40)
    errors = await debugger.capture_console_errors()
    print(f"Console Analysis: {errors}")
    
    print("\n🧩 STEP 4: Component Loading Diagnosis")
    print("-" * 40)
    component_status = await debugger.check_component_loading("simple-agent-selector")
    print(f"Component Status: {component_status}")
    
    if "ERROR" in str(component_status) or "not registered" in str(component_status):
        print("\n🔄 STEP 5: Component Recovery Attempt")
        print("-" * 40)
        reload_result = await debugger.force_component_reload("simple-agent-selector")
        print(f"Reload Result: {reload_result}")
        
        print("\n⏱️  Waiting 3 seconds for component registration...")
        await asyncio.sleep(3)
        
        final_status = await debugger.check_component_loading("simple-agent-selector")
        print(f"Final Status: {final_status}")
        
        print("\n📸 STEP 6: Post-Fix Screenshot")
        print("-" * 40)
        final_screenshot = await debugger.take_screenshot_with_validation()
        print(f"Final Screenshot: {final_screenshot}")
    
    print("\n" + "=" * 60)
    print("🏁 CLIENT DEBUG SESSION COMPLETE")
    print(f"🕐 Finished at: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    
    print("\n🎯 QA CHECKLIST:")
    print("✅ Connection established" if "Error" not in str(version_status) else "❌ Connection failed")
    print("✅ Screenshots working" if "Error" not in str(screenshot_result) else "❌ Screenshots failed")
    print("✅ Console access working" if "Error" not in str(errors) else "❌ Console access failed")
    
    print("\n📋 Next Actions:")
    print("1. Review console logs for specific error messages with line numbers")
    print("2. Check screenshots in ~/.continuum/screenshots/ directory")
    print("3. If components still not loading, check UIGenerator.cjs syntax")
    print("4. Verify browser tab is focused and responding")
    print("5. Consider server restart if persistent issues")

if __name__ == "__main__":
    asyncio.run(main())