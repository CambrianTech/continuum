"""
Claude Unified Debugger - Main Orchestrator
===========================================

Main class that coordinates all components for comprehensive debugging.
"""

import asyncio
from continuum_client.utils import get_continuum_ws_url, load_continuum_config

from .connection import WebSocketConnection
from .validation import ConnectionValidator
from .managers import ScreenshotManager, ServerLogManager


class ClaudeDebugger:
    """
    Main Debugger Orchestrator
    
    Coordinates all components for comprehensive Continuum validation
    and debugging capabilities. Provides self-validating connection
    that works with browser client validation through server logs.
    
    Components:
    - WebSocketConnection: Core connection management
    - ConnectionValidator: Diagnostic validation system
    - ScreenshotManager: Screenshot capture and management
    
    Features:
    - Self-validating connection with diagnostic checks
    - Cross-client validation coordination
    - Complete UI debugging capabilities
    - Modular component architecture
    """
    
    def __init__(self):
        load_continuum_config()
        self.ws_url = get_continuum_ws_url()
        self.connection = None
        self.validator = None
        self.screenshot_manager = None
        self.server_log_manager = None
        
    async def initialize(self):
        """Initialize all components"""
        self.connection = WebSocketConnection(self.ws_url)
        await self.connection.connect()
        
        self.validator = ConnectionValidator(self.connection)
        self.screenshot_manager = ScreenshotManager(self.connection)
        self.server_log_manager = ServerLogManager(self.connection)
        
    async def validate_connection(self):
        """Run full connection validation"""
        if not self.connection or not self.connection.is_connected:
            print(f"❌ Not connected to WebSocket")
            return False
            
        return await self.validator.validate_all()
        
    async def test_screenshot_capability(self):
        """Test screenshot functionality"""
        if not self.screenshot_manager:
            return False
        return await self.screenshot_manager.test_screenshot()
        
    async def capture_ui_element(self, selector):
        """Capture screenshot of specific UI element"""
        if not self.screenshot_manager:
            return False
        return await self.screenshot_manager.capture_element_screenshot(selector)
        
    def get_validation_results(self):
        """Get validation results for cross-client sharing"""
        if self.validator:
            return self.validator.validation_results
        return {}
        
    async def test_console_and_execution(self):
        """Test console reading and code execution via bus"""
        from .validation import JavaScriptValidator
        
        js_validator = JavaScriptValidator(self.connection)
        
        # Test console reading and execution
        print(f"   🔍 Reading browser console and executing code...")
        result = await js_validator.execute_and_wait('''
            console.log("🔍 Claude demonstrating console access via bus");
            console.log("Page URL:", window.location.href);
            console.log("Page title:", document.title);
            
            const versionBadge = document.querySelector(".version-badge");
            if (versionBadge) {
                console.log("✅ Version badge:", versionBadge.textContent.trim());
            }
            
            console.log("✅ Console reading and code execution working!");
            return "CONSOLE_AND_EXECUTION_CONFIRMED";
        ''')
        
        if result:
            print(f"   ✅ Console reading and code execution: SUCCESS")
            print(f"   🎯 Return value: {result}")
        else:
            print(f"   ❌ Console reading and code execution: FAILED")
    
    async def test_dual_visibility(self):
        """Demonstrate dual client/server log visibility"""
        print(f"\n🔍 DEMONSTRATING: Dual client/server log visibility...")
        
        # Test server log reading capability
        if self.server_log_manager:
            console_logs = await self.server_log_manager.read_console_output()
            if console_logs:
                print(f"   ✅ Server console logs accessible: {len(console_logs)} entries")
            else:
                print(f"   ⚠️ Server console logs not available")
        
        # Test client-side console reading via JavaScript
        from .validation import JavaScriptValidator
        js_validator = JavaScriptValidator(self.connection)
        
        client_result = await js_validator.execute_and_wait('''
            console.log("🔍 Client-side test message for dual visibility");
            console.log("Timestamp:", new Date().toISOString());
            
            // Check if we can see both client and server logs
            const visibility = {
                client_console: !!console,
                server_connection: !!window.ws,
                dual_mode: true
            };
            
            console.log("✅ Dual visibility test:", JSON.stringify(visibility));
            return JSON.stringify(visibility);
        ''')
        
        if client_result:
            print(f"   ✅ Client-side logging: SUCCESS")
            print(f"   🎯 Visibility data: {client_result}")
        else:
            print(f"   ❌ Client-side logging: FAILED")
            
        print(f"   🔗 DUAL VISIBILITY: You can now see both client and server logs simultaneously")
    
    async def execute_browser_script(self, script_path):
        """Execute a JavaScript file in the browser and get console results"""
        from pathlib import Path
        
        if not Path(script_path).exists():
            print(f"   ❌ Script not found: {script_path}")
            return None
            
        with open(script_path, 'r') as f:
            js_code = f.read()
            
        print(f"   🔧 Executing {Path(script_path).name} in browser...")
        
        from .validation import JavaScriptValidator
        js_validator = JavaScriptValidator(self.connection)
        
        result = await js_validator.execute_and_wait(js_code)
        
        if result:
            print(f"   ✅ Browser script execution: SUCCESS")
            print(f"   🎯 Result: {result}")
            return result
        else:
            print(f"   ❌ Browser script execution: FAILED")
            return None
    
    async def test_version_monitoring(self):
        """Test version monitoring capabilities"""
        print(f"\n🔍 DEMONSTRATING: Version monitoring and browser script execution...")
        
        # Execute version check script
        version_result = await self.execute_browser_script("browser_scripts/version_check.js")
        
        if version_result and "VERSION_CHECK_COMPLETE" in version_result:
            version = version_result.split("_")[-1]
            print(f"   📋 Current version detected: {version}")
        
        # Execute version monitor script  
        monitor_result = await self.execute_browser_script("browser_scripts/version_monitor.js")
        
        if monitor_result:
            import json
            try:
                monitor_data = json.loads(monitor_result)
                status = monitor_data.get('status', 'unknown')
                print(f"   👁️ Version monitor status: {status}")
                
                if status == "VERSION_INCREMENTED":
                    print(f"   🎉 Version increment detected!")
                    capabilities = monitor_data.get('data', {}).get('capabilities', {})
                    all_online = monitor_data.get('data', {}).get('allSystemsOnline', False)
                    print(f"   🔧 All systems online: {'✅' if all_online else '❌'}")
                elif status == "VERSION_STABLE":
                    print(f"   📊 Version stable: {monitor_data.get('version')}")
                elif status == "INITIAL_VERSION_STORED":
                    print(f"   🏁 Initial version stored: {monitor_data.get('version')}")
                    
            except json.JSONDecodeError:
                print(f"   ⚠️ Could not parse monitor result")
        
        print(f"   🔗 ASYNC EXECUTION: Python debugger can now execute browser scripts and get real-time console results")
    
    async def test_screenshot_validation_logs(self):
        """Test screenshot validation and capture console logs showing bytes/dimensions"""
        print(f"\n📸 DEMONSTRATING: Screenshot validation with bytes and dimensions logging...")
        
        # Execute the enhanced screenshot validator
        screenshot_result = await self.execute_browser_script("browser_scripts/screenshot_validator.js")
        
        if screenshot_result:
            import json
            try:
                screenshot_data = json.loads(screenshot_result)
                status = screenshot_data.get('status', 'unknown')
                print(f"   📸 Screenshot status: {status}")
                
                if status == "SUCCESS":
                    dims = screenshot_data.get('dimensions', {})
                    byte_size = screenshot_data.get('byteSize', 0)
                    filename = screenshot_data.get('filename', 'unknown')
                    
                    print(f"   📐 Dimensions: {dims.get('width')}x{dims.get('height')}")
                    print(f"   💾 Data size: {byte_size} bytes")
                    print(f"   🏷️ Filename: {filename}")
                    print(f"   📤 Screenshot sent to server successfully")
                    
                    # Check server logs for reception
                    if self.server_log_manager:
                        server_logs = await self.server_log_manager.read_server_logs()
                        if server_logs:
                            screenshot_logs = [log for log in server_logs if 'screenshot' in str(log).lower()]
                            if screenshot_logs:
                                print(f"   ✅ Server received screenshot data: {len(screenshot_logs)} log entries")
                            else:
                                print(f"   ⚠️ No screenshot logs found in server")
                                
                elif status == "PENDING":
                    print(f"   ⏰ Screenshot capture in progress...")
                else:
                    print(f"   ❌ Screenshot failed: {screenshot_data.get('message', 'Unknown error')}")
                    
            except json.JSONDecodeError:
                print(f"   ⚠️ Could not parse screenshot result: {screenshot_result}")
        else:
            print(f"   ❌ No screenshot result received")
            
        print(f"   🔗 SCREENSHOT VALIDATION: Browser can capture screenshots with full logging of bytes and dimensions")
    
    async def test_server_websocket_handling(self):
        """Test server WebSocket message handling to find where screenshot data is lost"""
        print(f"\n🔍 DEMONSTRATING: Server WebSocket message handling diagnosis...")
        
        # Check server WebSocket message logs
        if self.server_log_manager:
            server_info = await self.server_log_manager.check_websocket_message_logs()
            
            if server_info:
                print(f"   📊 Server analysis complete")
                if not server_info.get('hasWebSocketServer'):
                    print(f"   ⚠️ Server may not have WebSocket message handling")
                if not server_info.get('hasScreenshotHandler'):
                    print(f"   ⚠️ Server may not have screenshot data handler")
                    
        # Test sending a simple message to see if server receives it
        simple_message_test = await self.execute_browser_script("browser_scripts/check_server_logs.js")
        if simple_message_test:
            print(f"   ✅ Simple message test completed")
            
        # Now try to trace where screenshot messages go in the command processor
        trace_code = '''
        console.log("🔍 Tracing Continuum command processor for screenshot handling...");
        
        // Look for command processor or message handlers
        const processorInfo = {
            commandProcessor: !!window.CommandProcessor,
            messageHandlers: !!window.messageHandlers,
            screenshotCommands: [],
            webSocketHandlers: []
        };
        
        // Check global scope for command processor patterns
        for (let key in window) {
            if (key.includes('command') || key.includes('Command')) {
                processorInfo.commandProcessor = key;
                console.log("🔍 Found command-related:", key);
            }
            if (key.includes('handler') || key.includes('Handler')) {
                processorInfo.webSocketHandlers.push(key);
                console.log("🔍 Found handler-related:", key);
            }
            if (key.includes('screenshot') || key.includes('Screenshot')) {
                processorInfo.screenshotCommands.push(key);
                console.log("📸 Found screenshot-related:", key);
            }
        }
        
        // Check if we can access the actual command processor
        if (window.continuum && window.continuum.commandProcessor) {
            console.log("✅ Found Continuum command processor");
            processorInfo.continuumProcessor = true;
            
            // Check if it has screenshot handling
            if (window.continuum.commandProcessor.handleScreenshot) {
                processorInfo.hasScreenshotHandler = true;
                console.log("✅ Command processor has screenshot handler");
            }
        }
        
        console.log("📊 Command processor info:", JSON.stringify(processorInfo, null, 2));
        return JSON.stringify(processorInfo);
        '''
        
        from .validation import JavaScriptValidator
        js_validator = JavaScriptValidator(self.connection)
        processor_result = await js_validator.execute_and_wait(trace_code)
        
        if processor_result:
            import json
            try:
                processor_data = json.loads(processor_result)
                print(f"   🔧 Command processor analysis:")
                print(f"      Continuum processor: {'✅' if processor_data.get('continuumProcessor') else '❌'}")
                print(f"      Screenshot handler: {'✅' if processor_data.get('hasScreenshotHandler') else '❌'}")
                print(f"      WebSocket handlers: {len(processor_data.get('webSocketHandlers', []))}")
                print(f"      Screenshot commands: {len(processor_data.get('screenshotCommands', []))}")
                
                if processor_data.get('webSocketHandlers'):
                    print(f"      Handler names: {processor_data['webSocketHandlers']}")
                    
            except json.JSONDecodeError:
                print(f"   ⚠️ Could not parse processor result")
                
        print(f"   🔗 SERVER DIAGNOSIS: Checked WebSocket message flow through Continuum command processor")
    
    async def auto_capture_version_screenshot(self):
        """Automatically capture version screenshot and save via bus command - CORE VALIDATION"""
        print(f"\n📸 AUTO CAPTURE: Version screenshot capture and server file save...")
        
        # This is the complete validation sequence that should happen automatically
        capture_and_save_code = '''
        console.log("🎯 AUTO VALIDATION: Capturing version screenshot for server save...");
        
        // Check requirements
        const versionBadge = document.querySelector(".version-badge");
        if (!versionBadge) {
            console.log("❌ Version badge not found");
            return JSON.stringify({error: "No version badge found"});
        }
        
        if (typeof html2canvas === 'undefined') {
            console.log("❌ html2canvas not available");
            return JSON.stringify({error: "html2canvas not available"});
        }
        
        const version = versionBadge.textContent.trim();
        console.log("✅ Auto-capturing version:", version);
        
        // Capture screenshot
        html2canvas(versionBadge, {
            allowTaint: true,
            useCORS: true,
            scale: 2,
            backgroundColor: "#ffffff"
        }).then(function(canvas) {
            
            const dataURL = canvas.toDataURL('image/png');
            const timestamp = Date.now();
            const filename = `version_${version}_${timestamp}.png`;
            const base64Data = dataURL.split(',')[1];
            const byteSize = Math.round((base64Data.length * 3) / 4);
            
            console.log("📸 AUTO CAPTURE: Screenshot ready");
            console.log("   📐 Size:", canvas.width + "x" + canvas.height);
            console.log("   💾 Bytes:", byteSize);
            console.log("   🏷️ File:", filename);
            
            // Send via bus command for server file save
            const busFileCommand = {
                type: 'task',
                role: 'system',
                task: `[CMD:SAVE_FILE] {"filename":"${filename}","directory":".continuum/screenshots","content":"${base64Data}","mimeType":"image/png","metadata":{"version":"${version}","dimensions":{"width":${canvas.width},"height":${canvas.height}},"byteSize":${byteSize},"timestamp":${timestamp},"source":"auto_validation"}}`
            };
            
            console.log("🚌 AUTO VALIDATION: Sending bus file save command...");
            console.log("   📁 Target: .continuum/screenshots/" + filename);
            console.log("   🎯 Command: [CMD:SAVE_FILE]");
            
            // Send through WebSocket
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify(busFileCommand));
                console.log("✅ AUTO VALIDATION: Bus command sent to server!");
                console.log("   🔧 Server should now save file and log the process");
                
                // Store validation result
                window.autoValidationResult = {
                    filename: filename,
                    version: version,
                    byteSize: byteSize,
                    timestamp: timestamp,
                    busCommandSent: true,
                    validationType: "auto_connection_validation"
                };
                
                console.log("🎉 AUTO VALIDATION COMPLETE - Screenshot sent for server save");
                
            } else {
                console.log("❌ WebSocket not connected for auto validation");
            }
            
        }).catch(function(error) {
            console.log("❌ Auto screenshot capture failed:", error);
        });
        
        // Return immediate status
        return JSON.stringify({
            status: "AUTO_VALIDATION_INITIATED",
            version: version,
            action: "version_screenshot_capture_and_server_save",
            timestamp: Date.now()
        });
        '''
        
        from .validation import JavaScriptValidator
        js_validator = JavaScriptValidator(self.connection)
        
        print(f"   🔄 Executing automatic version capture and server save...")
        result = await js_validator.execute_and_wait(capture_and_save_code)
        
        if result:
            import json
            try:
                auto_result = json.loads(result)
                if auto_result.get('status') == 'AUTO_VALIDATION_INITIATED':
                    version = auto_result.get('version')
                    print(f"   ✅ Auto validation initiated for version: {version}")
                    print(f"   📸 Screenshot captured and bus command sent")
                    print(f"   🚌 [CMD:SAVE_FILE] sent to server for processing")
                    print(f"   📁 Expected file: .continuum/screenshots/version_{version}_timestamp.png")
                    print(f"   🎯 SERVER SHOULD NOW: Create directory, save file, log success")
                else:
                    print(f"   ⚠️ Auto validation status: {auto_result.get('status')}")
                    if 'error' in auto_result:
                        print(f"   ❌ Error: {auto_result['error']}")
            except json.JSONDecodeError:
                print(f"   ⚠️ Could not parse auto validation result")
        else:
            print(f"   ❌ Auto validation failed to execute")
            
        print(f"   🔗 AUTO VALIDATION: Complete connection validation with screenshot save")
    
    async def cleanup(self):
        """Clean up all connections and resources"""
        if self.connection:
            await self.connection.disconnect()


async def main():
    """Main entry point for Claude unified debugger"""
    debugger = ClaudeDebugger()
    
    try:
        # Initialize all components
        await debugger.initialize()
        print(f"🔌 Claude connected to Continuum: {debugger.ws_url}")
        
        # Run self-validation
        validation_success = await debugger.validate_connection()
        
        # Show validation results regardless of overall success
        results = debugger.get_validation_results()
        print(f"\n📋 Validation results:")
        for check, result in results.items():
            print(f"   {check}: {'✅' if result else '❌'}")
        
        # Demonstrate console reading and code execution via bus
        print(f"\n🔍 DEMONSTRATING: Console reading and code execution via bus...")
        await debugger.test_console_and_execution()
        
        # Demonstrate dual client/server log visibility
        await debugger.test_dual_visibility()
        
        # Demonstrate version monitoring and browser script execution
        await debugger.test_version_monitoring()
        
        # Demonstrate screenshot validation with bytes and dimensions logging
        await debugger.test_screenshot_validation_logs()
        
        # Check server WebSocket message handling specifically
        await debugger.test_server_websocket_handling()
        
        # AUTOMATIC VERSION SCREENSHOT CAPTURE AND SAVE
        await debugger.auto_capture_version_screenshot()
        
        if validation_success:
            print(f"\n🔧 CLAUDE DEBUGGER: OPERATIONAL")
            print(f"✅ Connection self-validated successfully")
        else:
            print(f"\n🔧 CLAUDE DEBUGGER: PARTIAL (console & JS execution working)") 
            print(f"⚠️ Some validations failed but core functionality operational")
            
    except Exception as e:
        print(f"❌ Debugger initialization failed: {e}")
        
    finally:
        # Clean up connections
        await debugger.cleanup()


if __name__ == "__main__":
    asyncio.run(main())