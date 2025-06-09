#!/usr/bin/env python3
"""
Claude Unified Debugger - Modular WebSocket Client Architecture
================================================================================

Organized class structure for comprehensive Continuum validation and debugging:

- WebSocketConnection: Core connection management  
- ConnectionValidator: Diagnostic validation checks
- JavaScriptValidator: Browser JS execution testing
- ScreenshotManager: Screenshot capture and validation
- ClaudeDebugger: Main orchestrator for all components

Provides self-validating connection that coordinates with browser client
validation through server logs and cross-client bus communication.
"""

import asyncio
import json
import base64
import websockets
from pathlib import Path
from continuum_client.utils import get_continuum_ws_url, load_continuum_config


class WebSocketConnection:
    """
    Core WebSocket Connection Management
    ===================================
    
    Handles low-level WebSocket connection, message sending/receiving,
    and connection lifecycle management for Continuum communication.
    
    Features:
    - Automatic connection establishment
    - JSON message serialization/deserialization  
    - Connection state tracking
    - Graceful cleanup and disconnection
    """
    
    def __init__(self, ws_url):
        self.ws_url = ws_url
        self.ws = None
        self.is_connected = False
        self.connection_id = None
        
    async def connect(self):
        """Establish WebSocket connection"""
        try:
            self.ws = await websockets.connect(self.ws_url)
            self.is_connected = True
            return True
        except Exception as e:
            print(f"❌ WebSocket connection failed: {e}")
            return False
            
    async def disconnect(self):
        """Close WebSocket connection"""
        if self.ws:
            await self.ws.close()
            self.is_connected = False
            
    async def send_message(self, message):
        """Send JSON message through WebSocket"""
        if not self.is_connected or not self.ws:
            raise ConnectionError("WebSocket not connected")
        await self.ws.send(json.dumps(message))
        
    async def receive_message(self, timeout=2):
        """Receive and parse JSON message"""
        if not self.is_connected or not self.ws:
            raise ConnectionError("WebSocket not connected")
        response = await asyncio.wait_for(self.ws.recv(), timeout=timeout)
        return json.loads(response)


class ConnectionValidator:
    """
    Connection Diagnostic Validation System
    ======================================
    
    Runs comprehensive validation checks to ensure all connection
    capabilities are working properly. Each connection validates itself
    and stores results for cross-client coordination.
    
    Validation Areas:
    - WebSocket connection stability
    - Command access and availability  
    - Browser JavaScript execution
    - Console message capture
    - Version detection and UI interaction
    
    Results are cached for cross-client validation sharing.
    """
    
    def __init__(self, connection):
        self.connection = connection
        self.validation_results = {}
        self.validation_timestamp = None
        
    async def validate_all(self):
        """Run all validation checks"""
        print(f"🔍 CONNECTION VALIDATION: Starting diagnostic checks...")
        
        checks = {
            'websocket_connection': self._validate_websocket,
            'command_access': self._validate_command_access,
            'browser_js_execution': self._validate_browser_js,
            'console_capture': self._validate_console_capture,
            'version_detection': self._validate_version_detection
        }
        
        results = {}
        
        for check_name, check_func in checks.items():
            try:
                results[check_name] = await check_func()
                status = "✅ PASS" if results[check_name] else "❌ FAIL"
                print(f"{status} - {check_name.replace('_', ' ').title()}")
            except Exception as e:
                results[check_name] = False
                print(f"❌ FAIL - {check_name.replace('_', ' ').title()}: {e}")
                
        self.validation_results = results
        
        passed = sum(results.values())
        total = len(results)
        success_rate = (passed / total) * 100
        
        print(f"\n🎯 VALIDATION SUMMARY: {passed}/{total} ({success_rate:.1f}%)")
        return success_rate >= 80
        
    async def _validate_websocket(self):
        """Check WebSocket connection"""
        return self.connection.is_connected
        
    async def _validate_command_access(self):
        """Check command access through banner"""
        try:
            # Skip status and get banner
            await self.connection.receive_message()
            banner = await self.connection.receive_message()
            
            if banner.get('type') == 'connection_banner':
                commands = banner.get('data', {}).get('commands', {}).get('available', [])
                print(f"   📋 Available commands: {len(commands)}")
                return len(commands) > 0
            return False
        except Exception:
            return False
            
    async def _validate_browser_js(self):
        """Check browser JavaScript execution"""
        try:
            js_validator = JavaScriptValidator(self.connection)
            return await js_validator.test_execution()
        except Exception:
            return False
            
    async def _validate_console_capture(self):
        """Check console message capture"""
        # This is tested as part of browser JS validation
        return self.validation_results.get('browser_js_execution', False)
        
    async def _validate_version_detection(self):
        """Check version badge detection"""
        # This is tested as part of browser JS validation  
        return self.validation_results.get('browser_js_execution', False)


class JavaScriptValidator:
    """Handles browser JavaScript execution validation"""
    
    def __init__(self, connection):
        self.connection = connection
        
    async def test_execution(self):
        """Test JavaScript execution with version detection"""
        js_code = '''
        console.log("🔧 Claude JavaScript validation");
        const versionBadge = document.querySelector(".version-badge");
        if (versionBadge) {
            console.log("✅ Version found:", versionBadge.textContent);
            return "VERSION_" + versionBadge.textContent.trim();
        }
        return "NO_VERSION_BADGE";
        '''
        
        try:
            encoded_js = base64.b64encode(js_code.encode()).decode()
            task = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await self.connection.send_message(task)
            
            for attempt in range(5):
                try:
                    result = await self.connection.receive_message(timeout=2)
                    
                    if result.get('type') == 'working':
                        continue
                    elif result.get('type') == 'result':
                        return self._process_browser_response(result)
                        
                except asyncio.TimeoutError:
                    continue
                    
            print(f"   ❌ JavaScript execution: TIMEOUT")
            return False
            
        except Exception as e:
            print(f"   ❌ JavaScript execution failed: {e}")
            return False
            
    def _process_browser_response(self, result):
        """Process browser response from JavaScript execution"""
        try:
            data = result.get('data', {})
            
            if 'result' in data and 'browserResponse' in data.get('result', {}):
                browser_response = data['result']['browserResponse']
                
                if browser_response.get('success'):
                    console_output = browser_response.get('output', [])
                    return_value = browser_response.get('result')
                    
                    print(f"   ✅ Console messages captured: {len(console_output)}")
                    
                    if return_value and 'VERSION_' in return_value:
                        version = return_value.split('_')[1] if '_' in return_value else 'unknown'
                        print(f"   ✅ Version detected: {version}")
                        
                    return True
                else:
                    print(f"   ❌ Browser execution failed")
                    return False
            else:
                print(f"   ⚠️ No browser response received")
                return False
                
        except Exception as e:
            print(f"   ❌ Response processing failed: {e}")
            return False


class ScreenshotManager:
    """Handles screenshot capture and validation"""
    
    def __init__(self, connection):
        self.connection = connection
        
    async def test_screenshot(self):
        """Test screenshot capability"""
        try:
            screenshot_task = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:SCREENSHOT] {}'
            }
            
            await self.connection.send_message(screenshot_task)
            
            for attempt in range(5):
                try:
                    result = await self.connection.receive_message(timeout=5)
                    
                    if result.get('type') == 'working':
                        continue
                    elif result.get('type') == 'result':
                        print(f"📸 Screenshot result received")
                        return True
                        
                except asyncio.TimeoutError:
                    continue
                    
            return False
            
        except Exception as e:
            print(f"❌ Screenshot test failed: {e}")
            return False


class ClaudeDebugger:
    """Main debugger class orchestrating all components"""
    
    def __init__(self):
        load_continuum_config()
        self.ws_url = get_continuum_ws_url()
        self.connection = None
        self.validator = None
        self.screenshot_manager = None
        
    async def initialize(self):
        """Initialize all components"""
        self.connection = WebSocketConnection(self.ws_url)
        await self.connection.connect()
        
        self.validator = ConnectionValidator(self.connection)
        self.screenshot_manager = ScreenshotManager(self.connection)
        
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
        
    def get_validation_results(self):
        """Get validation results for cross-client sharing"""
        if self.validator:
            return self.validator.validation_results
        return {}
        
    async def cleanup(self):
        """Clean up connections"""
        if self.connection:
            await self.connection.disconnect()
        
    async def connect_and_validate(self):
        """Connect to WebSocket server with full feature access"""
        print(f"🔌 Claude connecting to Continuum: {self.ws_url}")

async def main():
    """Claude unified debugger - modular self-validating connection"""
    debugger = ClaudeDebugger()
    
    try:
        # Initialize all components
        await debugger.initialize()
        print(f"🔌 Claude connected to Continuum: {debugger.ws_url}")
        
        # Run self-validation
        validation_success = await debugger.validate_connection()
        
        if validation_success:
            print(f"\n🔧 CLAUDE DEBUGGER: OPERATIONAL")
            print(f"✅ Connection self-validated successfully")
            print(f"✅ Ready for cross-client validation and UI debugging")
            
            # Show validation results for cross-client reference
            results = debugger.get_validation_results()
            print(f"\n📋 Validation results available for other clients:")
            for check, result in results.items():
                print(f"   {check}: {'✅' if result else '❌'}")
                
            # Test screenshot capability if connection is good
            print(f"\n📸 Testing screenshot capability...")
            screenshot_success = await debugger.test_screenshot_capability()
            if screenshot_success:
                print(f"✅ Screenshot capability confirmed")
            else:
                print(f"⚠️ Screenshot capability needs work")
                
        else:
            print(f"\n🔧 CLAUDE DEBUGGER: NEEDS WORK") 
            print(f"❌ Connection validation failed")
            print(f"💡 Check browser connection and Continuum server status")
            
    except Exception as e:
        print(f"❌ Debugger initialization failed: {e}")
        
    finally:
        # Clean up connections
        await debugger.cleanup()

if __name__ == "__main__":
    asyncio.run(main())