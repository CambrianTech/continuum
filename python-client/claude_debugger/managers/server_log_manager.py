"""
Server Log Management System
===========================

Connects to server logs to read console output and error messages.
"""

import asyncio
import json
import time
from pathlib import Path


class ServerLogManager:
    """
    Server Log Connection and Reading Manager
    
    Handles connection to server logs to read:
    - Browser console output
    - JavaScript execution logs  
    - Error messages and warnings
    - Cross-client coordination logs
    """
    
    def __init__(self, connection):
        self.connection = connection
        self.log_history = []
        self.log_listeners = []
        
    async def read_server_logs(self, filter_pattern=None, timeout=5):
        """Read recent server logs"""
        print(f"   📋 Reading server logs...")
        
        # Request server logs via bus command
        log_request = {
            'type': 'task',
            'role': 'system',
            'task': '[CMD:READ_LOGS] {}'
        }
        
        try:
            await self.connection.send_message(log_request)
            
            for attempt in range(timeout):
                try:
                    result = await self.connection.receive_message(timeout=1)
                    
                    if result.get('type') == 'working':
                        continue
                    elif result.get('type') == 'result':
                        data = result.get('data', {})
                        
                        if data.get('role') == 'BusCommand':
                            bus_result = data.get('result', {})
                            
                            if 'logs' in bus_result:
                                logs = bus_result['logs']
                                print(f"   ✅ Server logs read: {len(logs)} entries")
                                self.log_history.extend(logs)
                                return logs
                            else:
                                print(f"   ⚠️ No logs in bus result")
                                return []
                        else:
                            continue
                            
                except asyncio.TimeoutError:
                    continue
                    
            print(f"   ❌ Server logs read timeout")
            return []
            
        except Exception as e:
            print(f"   ❌ Server logs read error: {e}")
            return []
            
    async def check_websocket_message_logs(self):
        """Check server logs specifically for WebSocket message handling"""
        print(f"   🔍 Checking server WebSocket message logs...")
        
        js_code = '''
        console.log("🔍 Claude checking server WebSocket message handling");
        
        // Check if server has WebSocket message logging
        const serverInfo = {
            hasWebSocketServer: !!window.WebSocketServer,
            hasMessageHandlers: !!window.messageHandlers,
            hasScreenshotHandler: !!window.handleScreenshotData,
            recentMessages: []
        };
        
        // Look for recent WebSocket messages in global scope
        if (window.recentWebSocketMessages) {
            serverInfo.recentMessages = window.recentWebSocketMessages.slice(-5);
            console.log("✅ Found recent WebSocket messages:", serverInfo.recentMessages.length);
        } else {
            console.log("⚠️ No recent WebSocket messages found");
        }
        
        // Check if there's a message log or handler registry
        if (window.wsMessageLog) {
            const screenshotMessages = window.wsMessageLog.filter(msg => 
                msg.type === 'screenshot_data' || 
                (typeof msg === 'string' && msg.includes('screenshot'))
            );
            serverInfo.screenshotMessagesFound = screenshotMessages.length;
            console.log("📸 Screenshot messages in log:", screenshotMessages.length);
        }
        
        console.log("📊 Server WebSocket info:", JSON.stringify(serverInfo, null, 2));
        return JSON.stringify(serverInfo);
        '''
        
        try:
            from ..validation import JavaScriptValidator
            js_validator = JavaScriptValidator(self.connection)
            result = await js_validator.execute_and_wait(js_code)
            
            if result:
                import json
                server_info = json.loads(result)
                print(f"   📊 Server WebSocket status:")
                print(f"      WebSocket Server: {'✅' if server_info.get('hasWebSocketServer') else '❌'}")
                print(f"      Message Handlers: {'✅' if server_info.get('hasMessageHandlers') else '❌'}")
                print(f"      Screenshot Handler: {'✅' if server_info.get('hasScreenshotHandler') else '❌'}")
                print(f"      Recent Messages: {len(server_info.get('recentMessages', []))}")
                print(f"      Screenshot Messages: {server_info.get('screenshotMessagesFound', 0)}")
                return server_info
            else:
                print(f"   ❌ Could not check server WebSocket status")
                return None
                
        except Exception as e:
            print(f"   ❌ Server WebSocket check error: {e}")
            return None
            
    async def read_console_output(self, pattern="console"):
        """Read console output from server logs"""
        print(f"   📋 Reading console output from server logs...")
        
        js_code = '''
        console.log("📋 Claude reading server console logs");
        
        // Check if server exposes console history
        if (window.serverConsoleHistory) {
            const recentConsole = window.serverConsoleHistory.slice(-10);
            console.log("✅ Found server console history:", recentConsole.length);
            return JSON.stringify({
                found: true,
                entries: recentConsole,
                count: recentConsole.length
            });
        } else {
            console.log("⚠️ No server console history available");
            return JSON.stringify({
                found: false,
                message: "Server console history not available"
            });
        }
        '''
        
        try:
            from ..validation import JavaScriptValidator
            js_validator = JavaScriptValidator(self.connection)
            result = await js_validator.execute_and_wait(js_code)
            
            if result:
                console_data = json.loads(result)
                if console_data.get('found'):
                    print(f"   ✅ Console output found: {console_data.get('count', 0)} entries")
                    return console_data.get('entries', [])
                else:
                    print(f"   ⚠️ Console output not available")
                    return []
            else:
                print(f"   ❌ Console output read failed")
                return []
                
        except Exception as e:
            print(f"   ❌ Console output error: {e}")
            return []
            
    async def watch_for_errors(self, duration=10):
        """Watch server logs for errors in real-time"""
        print(f"   👁️ Watching for errors for {duration}s...")
        
        start_time = time.time()
        errors_found = []
        
        while time.time() - start_time < duration:
            try:
                # Check for recent errors via JavaScript
                js_code = '''
                console.log("👁️ Claude checking for recent errors");
                
                const errors = [];
                
                // Check for JavaScript errors
                if (window.recentErrors) {
                    errors.push(...window.recentErrors);
                }
                
                // Check console for error messages
                if (window.console && window.console.history) {
                    const errorMessages = window.console.history.filter(msg => 
                        msg.level === 'error' || msg.message.includes('ERROR')
                    );
                    errors.push(...errorMessages);
                }
                
                console.log("Errors found:", errors.length);
                return JSON.stringify({
                    timestamp: Date.now(),
                    errors: errors,
                    count: errors.length
                });
                '''
                
                from ..validation import JavaScriptValidator
                js_validator = JavaScriptValidator(self.connection)
                result = await js_validator.execute_and_wait(js_code)
                
                if result:
                    error_data = json.loads(result)
                    if error_data.get('count', 0) > 0:
                        new_errors = error_data.get('errors', [])
                        errors_found.extend(new_errors)
                        print(f"   🚨 Errors detected: {len(new_errors)} new errors")
                
                await asyncio.sleep(2)
                
            except Exception as e:
                print(f"   ⚠️ Error watching: {e}")
                await asyncio.sleep(1)
                
        print(f"   📊 Error watch complete: {len(errors_found)} total errors found")
        return errors_found
        
    def get_recent_logs(self, count=10):
        """Get recent log entries"""
        return self.log_history[-count:] if self.log_history else []