"""
Connection Diagnostic Validation System
=======================================

Comprehensive validation checks for connection capabilities.
"""

import asyncio
from .javascript_validator import JavaScriptValidator


class ConnectionValidator:
    """
    Connection Diagnostic Validation System
    
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