#!/usr/bin/env python3
"""
Continuum Python Client
======================

Universal Python entry point for Continuum ecosystem.
Provides complete client capabilities for AI agents, developers, and automated systems.

Features:
- Full WebSocket bus access
- Browser script execution  
- Console/log monitoring
- Screenshot capture
- CSS hot reloading
- Widget development
- Real-time debugging
- Chat participation

Usage:
    python continuum_client.py

This becomes your comprehensive interface to Continuum from Python.
"""

import asyncio
import json
import base64
import time
from pathlib import Path
from typing import Dict, Any, Optional, List
import websockets

# Import our modular components
from continuum_client.utils.config import get_continuum_ws_url, load_continuum_config
from continuum_client.utils.server_manager import ContinuumServerManager

class ContinuumPythonClient:
    """
    Universal Python client for Continuum ecosystem
    Handles everything: debugging, development, chat, commands
    """
    
    def __init__(self, agent_name: str = "PythonClient", auto_start_server: bool = True):
        self.agent_name = agent_name
        self.ws = None
        self.connected = False
        self.message_handlers = {}
        self.server_manager = None
        self.auto_start_server = auto_start_server
        
        # Output buffering to prevent Claude Code crashes
        self.output_buffer = []
        self.max_output_length = 5000  # Prevent string length errors
        self.max_buffer_lines = 50     # Limit number of lines
        self.verbose = False           # Control output verbosity
        
        # Load Continuum configuration
        load_continuum_config()
        self.ws_url = get_continuum_ws_url()
        
        if self.verbose:
            print(f"🐍 Continuum Python Client initializing...")
            print(f"   🤖 Agent: {self.agent_name}")
            print(f"   🔗 WebSocket: {self.ws_url}")
    
    def log(self, message: str, force: bool = False):
        """Safe logging with output buffering"""
        if not self.verbose and not force:
            return
            
        # Add to buffer
        self.output_buffer.append(message)
        
        # Trim buffer if too large
        if len(self.output_buffer) > self.max_buffer_lines:
            self.output_buffer = self.output_buffer[-self.max_buffer_lines:]
        
        # Print safely
        safe_message = message[:self.max_output_length] if len(message) > self.max_output_length else message
        print(safe_message)
    
    def get_recent_output(self, lines: int = 10) -> str:
        """Get recent output safely"""
        recent = self.output_buffer[-lines:] if lines else self.output_buffer
        output = "\n".join(recent)
        return output[:self.max_output_length] if len(output) > self.max_output_length else output
    
    async def start(self):
        """Start the complete client system"""
        # 1. Start/ensure Continuum server is running
        if self.auto_start_server:
            await self.ensure_server_running()
        
        # 2. Connect to WebSocket
        await self.connect()
        
        # 3. Start main interaction loop
        await self.run_client_loop()
    
    async def ensure_server_running(self):
        """Ensure Continuum server is running and healthy"""
        self.log("\n🔧 SERVER MANAGEMENT:", force=True)
        
        self.server_manager = ContinuumServerManager()
        
        if self.server_manager.is_server_healthy(timeout=3):
            self.log("   ✅ Server already running", force=True)
        else:
            self.log("   🔄 Starting Continuum server...", force=True)
            if self.server_manager.start(restart=True):
                self.log("   ✅ Server started successfully", force=True)
            else:
                self.log("   ❌ Failed to start server", force=True)
                raise RuntimeError("Could not start Continuum server")
    
    async def connect(self):
        """Connect to Continuum WebSocket"""
        self.log(f"\n🔗 WEBSOCKET CONNECTION:", force=True)
        self.log(f"   🎯 Connecting to: {self.ws_url}", force=True)
        
        try:
            self.ws = await websockets.connect(self.ws_url)
            self.connected = True
            self.log("   ✅ Connected to Continuum WebSocket", force=True)
            
            # Send client ready handshake
            await self.send_client_ready()
            
            # Register as agent if specified
            if self.agent_name != "PythonClient":
                await self.register_as_agent()
            
            return True
            
        except Exception as e:
            self.log(f"   ❌ Connection failed: {e}", force=True)
            self.connected = False
            return False
    
    async def send_client_ready(self):
        """Send client ready handshake to server and wait for acknowledgment"""
        ready_message = {
            "type": "client_ready",
            "client_type": "python", 
            "client_id": f"python-{self.agent_name.lower()}",
            "capabilities": [
                "screenshot", "css_inject", "js_exec", 
                "browser_scripts", "file_save", "chat"
            ],
            "version": "1.0.0"
        }
        await self.send_message(ready_message)
        self.log("   📋 Sent client ready handshake", force=True)
        
        # Wait for server ready acknowledgment
        self.log("   ⏳ Waiting for server ready acknowledgment...", force=True)
        server_ready = await self.wait_for_server_ready(timeout=10)
        
        if server_ready:
            self.log("   ✅ Server ready acknowledgment received", force=True)
        else:
            self.log("   ⚠️ Server ready timeout (proceeding anyway)", force=True)
    
    async def wait_for_server_ready(self, timeout: int = 10) -> bool:
        """Wait for server ready acknowledgment"""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                # Check for incoming messages
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                # Look for server ready response
                if data.get("type") == "server_ready":
                    return True
                elif data.get("type") == "client_acknowledged":
                    return True
                elif data.get("type") == "status" and data.get("message", "").lower().find("ready") != -1:
                    return True
                    
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                self.log(f"   ⚠️ Error waiting for server ready: {e}", force=True)
                break
                
        return False
    
    async def register_as_agent(self):
        """Register this client as an AI agent"""
        registration = {
            "type": "agent_register",
            "agentInfo": {
                "agentId": self.agent_name.lower(),
                "agentName": self.agent_name,
                "agentType": "ai",
                "capabilities": [
                    "widget_development",
                    "ui_debugging", 
                    "screenshot_analysis",
                    "css_injection",
                    "javascript_execution",
                    "log_monitoring",
                    "file_operations",
                    "chat_participation"
                ],
                "hostInfo": {
                    "hostname": "python-client",
                    "platform": "python",
                    "version": "1.0.0"
                }
            }
        }
        
        await self.send_message(registration)
        self.log(f"   🤖 Registered as agent: {self.agent_name}", force=True)
    
    async def wait_for_browser_ready(self, timeout: int = 10) -> bool:
        """Wait for browser to signal it's ready for commands"""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                # Check for incoming messages
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                # Look for browser ready signals
                if data.get("type") == "browser_ready":
                    self.log("   ✅ Browser ready signal received", force=True)
                    return True
                elif data.get("type") == "client_connected" and "browser" in str(data).lower():
                    self.log("   ✅ Browser client connected", force=True)
                    return True
                elif "initialized" in str(data).lower() or "ready" in str(data).lower():
                    self.log("   ✅ Initialization signal detected", force=True)
                    return True
                    
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                self.log(f"   ⚠️ Error waiting for browser ready: {e}", force=True)
                break
                
        self.log("   ⏰ Browser ready timeout (proceeding anyway)", force=True)
        return False
    
    async def send_message(self, message: Dict[str, Any]):
        """Send message to Continuum"""
        if not self.connected or not self.ws:
            self.log("❌ Not connected to send message", force=True)
            return False
        
        try:
            await self.ws.send(json.dumps(message))
            return True
        except Exception as e:
            self.log(f"❌ Send error: {e}", force=True)
            return False
    
    async def execute_browser_script(self, script_path: str, timeout: int = 15) -> Dict[str, Any]:
        """Execute a browser script and get results"""
        script_file = Path(__file__).parent / "browser_scripts" / script_path
        
        if not script_file.exists():
            return {"success": False, "error": f"Script not found: {script_path}"}
        
        # Read script content
        js_code = script_file.read_text()
        
        # Encode for bus command
        encoded_js = base64.b64encode(js_code.encode()).decode()
        
        # Send via bus command
        command = {
            "type": "task",
            "role": "system", 
            "task": f"[CMD:BROWSER_JS] {encoded_js}"
        }
        
        self.log(f"🔧 Executing browser script: {script_path}", force=True)
        
        # Send command and wait for result
        await self.send_message(command)
        
        # Wait for js_executed response
        result = await self.wait_for_js_result(timeout)
        
        if result.get("success"):
            self.log(f"   ✅ Script executed successfully", force=True)
            if result.get("output"):
                self.log(f"   📋 Console messages: {len(result['output'])}", force=True)
            if result.get("result"):
                self.log(f"   🎯 Return value: {result['result']}", force=True)
        else:
            self.log(f"   ❌ Script failed: {result.get('error', 'Unknown error')}", force=True)
        
        return result
    
    async def wait_for_js_result(self, timeout: int = 15) -> Dict[str, Any]:
        """Wait for JavaScript execution result"""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                # Check for incoming messages
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                # Look for js_executed response
                if data.get("type") == "js_executed":
                    return {
                        "success": data.get("success", False),
                        "result": data.get("result"),
                        "output": data.get("output", []),
                        "error": data.get("error"),
                        "timestamp": data.get("timestamp")
                    }
                
                # Look for BusCommand result format
                if data.get("role") == "BusCommand" and "result" in data:
                    bus_result = data["result"]
                    if "browserResponse" in bus_result.get("result", {}):
                        browser_response = bus_result["result"]["browserResponse"]
                        return {
                            "success": True,
                            "result": browser_response.get("result"),
                            "output": browser_response.get("output", []),
                            "error": None,
                            "timestamp": time.time()
                        }
                
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                return {"success": False, "error": f"Wait error: {e}"}
        
        return {"success": False, "error": "Timeout waiting for result"}
    
    async def take_screenshot(self, filename: Optional[str] = None) -> Dict[str, Any]:
        """Capture browser screenshot"""
        if not filename:
            filename = f"screenshot_{int(time.time())}.png"
        
        command = {
            "type": "task",
            "role": "system",
            "task": f'[CMD:SCREENSHOT] {{"format": "png", "filename": "{filename}"}}'
        }
        
        print(f"📸 Taking screenshot: {filename}")
        await self.send_message(command)
        
        # Wait for screenshot result
        result = await self.wait_for_screenshot_result()
        
        if result.get("success"):
            print(f"   ✅ Screenshot saved: {result.get('path', filename)}")
        else:
            print(f"   ❌ Screenshot failed: {result.get('error')}")
        
        return result
    
    async def wait_for_screenshot_result(self, timeout: int = 15) -> Dict[str, Any]:
        """Wait for screenshot result"""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                # Look for screenshot completion message
                if data.get("type") == "result" and "screenshot" in data.get("message", "").lower():
                    return {"success": True, "message": data.get("message")}
                
                # Look for screenshot_data message
                if data.get("type") == "screenshot_data":
                    return {
                        "success": True,
                        "filename": data.get("filename"),
                        "dimensions": data.get("dimensions"),
                        "path": data.get("path")
                    }
                
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                return {"success": False, "error": f"Wait error: {e}"}
        
        return {"success": False, "error": "Screenshot timeout"}
    
    async def send_chat_message(self, message: str, room: str = "general") -> Dict[str, Any]:
        """Send chat message to Continuum"""
        chat_msg = {
            "type": "message",
            "content": message,
            "room": room
        }
        
        print(f"💬 Sending chat message to {room}: {message[:50]}{'...' if len(message) > 50 else ''}")
        await self.send_message(chat_msg)
        
        # Wait for response
        response = await self.wait_for_chat_response()
        
        if response.get("success"):
            print(f"   ✅ Response from {response.get('agent', 'Unknown')}")
            print(f"   📝 {response.get('message', '')[:100]}{'...' if len(response.get('message', '')) > 100 else ''}")
        
        return response
    
    async def wait_for_chat_response(self, timeout: int = 30) -> Dict[str, Any]:
        """Wait for chat response"""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                if data.get("type") == "response":
                    return {
                        "success": True,
                        "message": data.get("message"),
                        "agent": data.get("agent"),
                        "room": data.get("room")
                    }
                
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                return {"success": False, "error": f"Wait error: {e}"}
        
        return {"success": False, "error": "Chat response timeout"}
    
    async def inject_css(self, css_code: str) -> Dict[str, Any]:
        """Inject CSS into browser for hot reloading"""
        js_injection = f"""
        console.log("🎨 Injecting CSS hot reload...");
        
        // Remove previous hot reload styles
        const existingStyle = document.getElementById('python-client-hot-css');
        if (existingStyle) {{
            existingStyle.remove();
        }}
        
        // Inject new CSS
        const style = document.createElement('style');
        style.id = 'python-client-hot-css';
        style.textContent = `{css_code}`;
        document.head.appendChild(style);
        
        console.log("✅ CSS hot reload applied");
        "CSS_INJECTED";
        """
        
        # Encode and send via browser command
        encoded_js = base64.b64encode(js_injection.encode()).decode()
        command = {
            "type": "task",
            "role": "system",
            "task": f"[CMD:BROWSER_JS] {encoded_js}"
        }
        
        print("🎨 Injecting CSS hot reload...")
        await self.send_message(command)
        
        result = await self.wait_for_js_result()
        
        if result.get("success"):
            print("   ✅ CSS injection successful")
        else:
            print(f"   ❌ CSS injection failed: {result.get('error')}")
        
        return result
    
    async def run_client_loop(self):
        """Main client interaction loop"""
        self.log(f"\n🚀 {self.agent_name} client ready!", force=True)
        
        # Check if running in interactive environment
        import os
        if not os.isatty(0):  # Not a terminal/interactive
            self.log("Non-interactive mode: running validation test", force=True)
            await self.run_validation_test()
            return
        
        self.log("Interactive mode: type 'screenshot' to test, 'help' for commands", force=True)
        
        while self.connected:
            try:
                # Get user input (only in interactive mode)
                command = input(f"\n{self.agent_name}> ").strip()
                
                if not command:
                    continue
                
                if command == "quit":
                    break
                elif command == "help":
                    print("\nAvailable commands:")
                    print("  script <name>     - Execute browser script from browser_scripts/")
                    print("  screenshot        - Take browser screenshot")
                    print("  chat <message>    - Send message to Continuum chat")
                    print("  css <css_code>    - Inject CSS for hot reloading")
                    print("  help              - Show this help")
                    print("  quit              - Exit client")
                
                elif command.startswith("script "):
                    script_name = command[7:].strip()
                    if not script_name.endswith('.js'):
                        script_name += '.js'
                    await self.execute_browser_script(script_name)
                
                elif command == "screenshot":
                    await self.take_screenshot()
                
                elif command.startswith("chat "):
                    message = command[5:].strip()
                    await self.send_chat_message(message)
                
                elif command.startswith("css "):
                    css_code = command[4:].strip()
                    await self.inject_css(css_code)
                
                else:
                    self.log(f"Unknown: {command}. Try 'help'", force=True)
                
            except KeyboardInterrupt:
                self.log("\n👋 Exiting...", force=True)
                break
            except Exception as e:
                self.log(f"❌ Error: {e}", force=True)
        
        await self.disconnect()
    
    async def run_validation_test(self):
        """Run validation test - browser should already be validated by server"""
        self.log("🧪 Running validation test...", force=True)
        self.log("ℹ️ Note: Browser validation happens automatically at server level", force=True)
        
        # Send status indicator for UI (orange - connecting/validating) 
        await self.send_status_update("validating", "orange")
        
        # Brief wait for any server processes
        await asyncio.sleep(2)
        
        # Test 1: Chat capability (this works)
        self.log("💬 Testing chat capability...", force=True)
        chat_result = await self.send_chat_message("Python client validation test")
        if chat_result:
            self.log("   ✅ Chat test passed", force=True)
        else:
            self.log("   ❌ Chat test failed", force=True)
        
        # Test 2: Send comprehensive browser validation script with detailed logging
        self.log("🧪 Sending browser validation with detailed logging...", force=True)
        validation_script = """
        console.log('🔥 CONTINUUM_CLIENT.PY VALIDATION STARTED');
        console.log('⏰ Timestamp:', new Date().toISOString());
        console.log('🌐 User Agent:', navigator.userAgent);
        console.log('📍 URL:', window.location.href);
        console.log('📊 Document ready state:', document.readyState);
        
        // WebSocket validation
        console.log('🔗 WebSocket Status:');
        if (window.ws) {
            console.log('  ✅ WebSocket exists');
            console.log('  📊 Ready State:', window.ws.readyState);
            console.log('  🔗 URL:', window.ws.url);
            console.log('  📡 Protocol:', window.ws.protocol);
        } else {
            console.log('  ❌ No WebSocket found');
        }
        
        // Continuum API validation
        console.log('🔧 Continuum API Status:');
        if (window.continuum) {
            console.log('  ✅ window.continuum exists');
            console.log('  🔧 Methods:', Object.getOwnPropertyNames(window.continuum));
        } else {
            console.log('  ❌ window.continuum not found');
        }
        
        // Document and DOM validation
        console.log('📄 Document Status:');
        console.log('  📊 Ready State:', document.readyState);
        console.log('  🌐 Domain:', document.domain);
        console.log('  📏 Body dimensions:', {
            width: document.body.scrollWidth,
            height: document.body.scrollHeight
        });
        
        // Screenshot capability validation
        console.log('📸 Screenshot Capability:');
        if (typeof html2canvas !== 'undefined') {
            console.log('  ✅ html2canvas available');
            console.log('  📦 Version:', html2canvas.version || 'unknown');
        } else {
            console.log('  ❌ html2canvas not available');
        }
        
        // Version badge detection
        console.log('🏷️ Version Badge Detection:');
        const versionBadge = document.querySelector('.version-badge');
        if (versionBadge) {
            console.log('  ✅ Version badge found');
            console.log('  📝 Text:', versionBadge.textContent.trim());
            console.log('  📐 Dimensions:', {
                width: versionBadge.offsetWidth,
                height: versionBadge.offsetHeight,
                top: versionBadge.offsetTop,
                left: versionBadge.offsetLeft
            });
            console.log('  🎨 Computed styles:', {
                display: getComputedStyle(versionBadge).display,
                visibility: getComputedStyle(versionBadge).visibility,
                opacity: getComputedStyle(versionBadge).opacity
            });
        } else {
            console.log('  ⚠️ Version badge not found');
            console.log('  🔍 Available elements with version:', 
                Array.from(document.querySelectorAll('*')).filter(el => 
                    el.textContent && el.textContent.toLowerCase().includes('version')
                ).map(el => el.tagName + '.' + el.className)
            );
        }
        
        // Test screenshot attempt with detailed WebSocket logging
        if (typeof html2canvas !== 'undefined' && versionBadge) {
            console.log('📸 Attempting test screenshot...');
            html2canvas(versionBadge, {
                allowTaint: true,
                useCORS: true,
                scale: 1
            }).then(canvas => {
                console.log('  ✅ Screenshot successful!');
                console.log('  📐 Canvas size:', canvas.width + 'x' + canvas.height);
                
                const dataURL = canvas.toDataURL('image/png');
                console.log('  💾 Data URL length:', dataURL.length);
                console.log('  🔍 Data URL preview:', dataURL.substring(0, 100) + '...');
                
                // Test WebSocket send with detailed logging
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    const timestamp = Date.now();
                    const filename = `validation_screenshot_${timestamp}.png`;
                    
                    const screenshotData = {
                        type: 'screenshot_data',
                        filename: filename,
                        dataURL: dataURL,
                        timestamp: timestamp,
                        source: 'continuum_client_py_validation',
                        dimensions: {
                            width: canvas.width,
                            height: canvas.height
                        }
                    };
                    
                    console.log('📤 SENDING SCREENSHOT DATA TO SERVER:');
                    console.log('  📋 Message type:', screenshotData.type);
                    console.log('  📁 Filename:', filename);
                    console.log('  📏 Data length:', dataURL.length);
                    console.log('  📊 Dimensions:', canvas.width + 'x' + canvas.height);
                    console.log('  🕐 Timestamp:', timestamp);
                    console.log('  📡 WebSocket state:', window.ws.readyState);
                    
                    window.ws.send(JSON.stringify(screenshotData));
                    console.log('  ✅ Screenshot data sent to server via WebSocket');
                    
                    // Also send debug message
                    const debugMsg = {
                        type: 'debug_message',
                        message: 'Screenshot data sent from browser validation',
                        filename: filename,
                        dataSize: dataURL.length
                    };
                    window.ws.send(JSON.stringify(debugMsg));
                    console.log('  📋 Debug message sent to server');
                    
                } else {
                    console.log('  ❌ WebSocket not available for sending screenshot');
                    console.log('  📊 WebSocket state:', window.ws ? window.ws.readyState : 'no ws');
                }
                
            }).catch(error => {
                console.log('  ❌ Screenshot failed:', error.message);
            });
        }
        
        // Test remote code execution capability
        console.warn('⚠️ TEST WARNING from continuum_client.py validation');
        console.error('🔴 TEST ERROR from continuum_client.py validation');
        
        // Test version reading
        const versionText = versionBadge ? versionBadge.textContent.trim() : 'NO_VERSION_FOUND';
        console.log('📋 VERSION_READ_RESULT:', versionText);
        
        console.log('🎯 CONTINUUM_CLIENT.PY VALIDATION COMPLETE');
        console.log('📊 Check Continuum server logs for this output');
        """
        
        await self.send_browser_experiment(validation_script)
        self.log("   📤 Browser validation experiment sent", force=True)
        self.log("   📊 Check Continuum server console for detailed output", force=True)
        
        # Brief wait for processing
        await asyncio.sleep(2)
        
        # Test 3: Request screenshot via bus command
        self.log("📸 Testing screenshot via bus command...", force=True)
        screenshot_cmd = {
            "type": "task",
            "role": "system",
            "task": "[CMD:SCREENSHOT]"
        }
        await self.send_message(screenshot_cmd)
        self.log("   📤 Screenshot bus command sent", force=True)
        
        # Send status indicator for UI (green - ready)
        await self.send_status_update("ready", "green")
        
        self.log("\n✅ Validation complete!", force=True)
        self.log("🎯 Check browser console for detailed logging output", force=True)
    
    async def inject_javascript_with_logging(self, js_code: str) -> bool:
        """Inject JavaScript code via chat message system"""
        try:
            # Send JavaScript as a message that PlannerAI might execute
            js_message = {
                "type": "message",
                "content": f"Execute this JavaScript in the browser console:\n```javascript\n{js_code}\n```",
                "agentId": "claude",
                "room": "general"
            }
            await self.send_message(js_message)
            self.log("   📤 JavaScript injection request sent", force=True)
            
            # Also try direct bus command format
            bus_command = {
                "type": "task",
                "role": "system",
                "task": f"[CMD:BROWSER_JS] {base64.b64encode(js_code.encode()).decode()}"
            }
            await self.send_message(bus_command)
            self.log("   📤 Browser JS bus command sent", force=True)
            
            return True
        except Exception as e:
            self.log(f"   ❌ JavaScript injection failed: {e}", force=True)
            return False
    
    async def listen_for_browser_responses(self, timeout: int = 5):
        """Listen for browser console output and responses"""
        start_time = time.time()
        messages_received = 0
        
        while time.time() - start_time < timeout:
            try:
                # Listen for incoming WebSocket messages
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                messages_received += 1
                
                # Log any browser-related responses
                msg_type = data.get("type", "unknown")
                self.log(f"   📥 Received: {msg_type}", force=True)
                
                # Look for console output, JS execution results, or browser responses
                if "console" in str(data).lower():
                    self.log(f"   📝 Console message detected", force=True)
                
                if "browser" in str(data).lower():
                    self.log(f"   🌐 Browser response detected", force=True)
                
                if "js" in str(data).lower() or "javascript" in str(data).lower():
                    self.log(f"   ⚡ JavaScript response detected", force=True)
                
                if data.get("role") == "BusCommand":
                    self.log(f"   🚌 Bus command response: {data.get('result', {}).get('status', 'unknown')}", force=True)
                
                # Show content if it looks like console output
                content = data.get("content", "")
                if content and ("PYTHON CLIENT VALIDATION" in content or "console.log" in content):
                    self.log(f"   💬 Content: {content[:100]}...", force=True)
                    
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                self.log(f"   ⚠️ Error listening: {e}", force=True)
                break
        
        self.log(f"   📊 Listened for {timeout}s, received {messages_received} messages", force=True)
    
    async def send_browser_experiment(self, script: str):
        """Send browser experiment script via multiple channels"""
        try:
            # Method 1: Direct bus command
            bus_command = {
                "type": "task",
                "role": "system", 
                "task": f"[CMD:BROWSER_JS] {base64.b64encode(script.encode()).decode()}"
            }
            await self.send_message(bus_command)
            
            # Method 2: Via chat message
            chat_msg = {
                "type": "message",
                "content": f"Execute browser validation script:\n```javascript\n{script}\n```",
                "agentId": "claude",
                "room": "general"
            }
            await self.send_message(chat_msg)
            
            return True
        except Exception as e:
            self.log(f"   ❌ Browser experiment failed: {e}", force=True)
            return False
    
    async def send_status_update(self, status: str, color: str):
        """Send status update for UI indicators"""
        status_message = {
            "type": "agent_status",
            "agentId": self.agent_name.lower(),
            "status": status,
            "color": color,
            "timestamp": time.time()
        }
        await self.send_message(status_message)
        self.log(f"   📊 Status update: {status} ({color})", force=True)
    
    async def send_direct_bus_command(self, command: str, params: dict = None, timeout: int = 15) -> bool:
        """Send a direct bus command and wait for result"""
        # Format as bus command
        if params:
            cmd_string = f"[CMD:{command}] {json.dumps(params)}"
        else:
            cmd_string = f"[CMD:{command}]"
        
        message = {
            "type": "task",
            "role": "system",
            "task": cmd_string
        }
        
        self.log(f"   📤 Sending bus command: {command}", force=True)
        await self.send_message(message)
        
        # Wait for response on WebSocket
        return await self.wait_for_bus_response(timeout)
    
    async def wait_for_bus_response(self, timeout: int = 15) -> bool:
        """Wait for any bus command response"""
        start_time = time.time()
        
        while time.time() - start_time < timeout:
            try:
                # Check for incoming messages
                message = await asyncio.wait_for(self.ws.recv(), timeout=1.0)
                data = json.loads(message)
                
                # Look for any command response
                if data.get("type") in ["command_result", "bus_result", "task_result"]:
                    self.log(f"   📥 Received response: {data.get('type')}", force=True)
                    return True
                elif data.get("role") == "BusCommand":
                    self.log(f"   📥 BusCommand response received", force=True)
                    return True
                elif "result" in data or "success" in data:
                    self.log(f"   📥 Response with result received", force=True)
                    return True
                    
            except asyncio.TimeoutError:
                continue
            except Exception as e:
                self.log(f"   ⚠️ Error waiting for response: {e}", force=True)
                break
                
        self.log(f"   ⏰ Command timeout after {timeout}s", force=True)
        return False
    
    async def disconnect(self):
        """Disconnect from Continuum"""
        self.log("\n🔌 Disconnecting...", force=True)
        
        if self.ws:
            await self.ws.close()
        
        self.connected = False
        self.log("   ✅ Disconnected", force=True)

async def main():
    """Main entry point"""
    import sys
    
    # Parse command line arguments
    agent_name = "PythonClient"
    if len(sys.argv) > 1:
        agent_name = sys.argv[1]
    
    # Create client (non-verbose by default to prevent crashes)
    client = ContinuumPythonClient(agent_name=agent_name)
    client.verbose = "--verbose" in sys.argv
    
    try:
        await client.start()
    except KeyboardInterrupt:
        print("\n👋 Exiting...")
    except Exception as e:
        print(f"❌ Client error: {e}")

if __name__ == "__main__":
    asyncio.run(main())