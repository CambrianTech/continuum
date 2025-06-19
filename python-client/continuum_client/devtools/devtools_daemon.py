"""
DevTools Daemon - Object-Oriented DevTools Monitoring
Inherits from BaseDaemon for proper OOP structure
"""

import asyncio
from typing import Dict, Any, Optional
from ..core.daemon_manager import BaseDaemon
from .log_monitor import DevToolsLogMonitor


class DevToolsDaemon(BaseDaemon):
    """Object-oriented DevTools monitoring daemon"""
    
    def __init__(self, target_url: str = "localhost:9000", ports: list = None):
        super().__init__("devtools")
        
        self.target_url = target_url
        self.ports = ports or [9222, 9223]
        self.devtools_monitor = None
        self.browser_connected = False
        self.connection_attempts = 0
        self.log_count = 0
        
    async def handle_browser_log(self, log_entry: Dict[str, Any]):
        """Handle browser console log from DevTools"""
        self.log_count += 1
        
        # Log to daemon file with structured data
        self.write_log("BROWSER_CONSOLE", log_entry['text'], {
            'level': log_entry['level'],
            'timestamp': log_entry['timestamp'],
            'source': 'browser',
            'log_number': self.log_count
        })
        
        # Print for immediate feedback
        timestamp = log_entry['timestamp']
        level = log_entry['level'].upper()
        text = log_entry['text']
        print(f"🌐 [{timestamp}] {level}: {text}")
    
    async def attempt_connection(self) -> bool:
        """Attempt to connect to DevTools on available ports"""
        self.connection_attempts += 1
        
        self.write_log("CONNECTION_ATTEMPT", f"Attempting DevTools connection (attempt {self.connection_attempts})")
        
        for port in self.ports:
            try:
                self.write_log("TRYING_PORT", f"Trying DevTools port {port}")
                
                self.devtools_monitor = DevToolsLogMonitor(
                    chrome_port=port,
                    target_url=self.target_url,
                    log_callback=self.handle_browser_log
                )
                
                success = await self.devtools_monitor.connect()
                if success:
                    self.browser_connected = True
                    self.write_log("CONNECTED", f"DevTools connected successfully on port {port}", {
                        'port': port,
                        'target_url': self.target_url,
                        'attempt_number': self.connection_attempts
                    })
                    print(f"🔌 DevTools Daemon: Connected on port {port}")
                    return True
                    
            except Exception as e:
                self.write_log("CONNECTION_ERROR", f"Port {port} failed: {e}", {
                    'port': port,
                    'error': str(e)
                })
                continue
        
        self.write_log("CONNECTION_FAILED", f"All ports failed on attempt {self.connection_attempts}")
        return False
    
    async def monitor_connection(self):
        """Monitor and maintain DevTools connection"""
        while self.running:
            try:
                # Check connection health
                if not self.devtools_monitor or not self.devtools_monitor.connected:
                    self.browser_connected = False
                    self.write_log("CONNECTION_LOST", "DevTools connection lost, attempting reconnection")
                    
                    success = await self.attempt_connection()
                    if not success:
                        self.write_log("RECONNECT_WAIT", "Connection failed, waiting 30s before retry")
                        await asyncio.sleep(30)
                        continue
                
                # Health check every 10 seconds
                await asyncio.sleep(10)
                
                if self.browser_connected:
                    self.write_log("HEALTH_CHECK", "DevTools connection healthy", {
                        'logs_captured': self.log_count
                    })
                
            except Exception as e:
                self.write_log("MONITOR_ERROR", f"Connection monitoring error: {e}")
                await asyncio.sleep(30)
    
    async def capture_screenshot(self) -> Optional[str]:
        """Capture screenshot via DevTools Protocol"""
        if not self.devtools_monitor or not self.devtools_monitor.connected:
            self.write_log("SCREENSHOT_ERROR", "Cannot capture screenshot - DevTools not connected")
            return None
            
        try:
            self.write_log("SCREENSHOT_START", "Capturing screenshot via DevTools Protocol")
            
            # Use DevTools Protocol to capture screenshot
            screenshot_data = await self.devtools_monitor._send_command("Page.captureScreenshot", {
                "format": "png",
                "quality": 90
            })
            
            if screenshot_data and 'result' in screenshot_data:
                screenshot_b64 = screenshot_data['result'].get('data')
                
                self.write_log("SCREENSHOT_SUCCESS", f"Screenshot captured ({len(screenshot_b64)} bytes)", {
                    'format': 'png',
                    'quality': 90,
                    'size_bytes': len(screenshot_b64)
                })
                
                return screenshot_b64
            else:
                self.write_log("SCREENSHOT_ERROR", "Screenshot data not received")
                
        except Exception as e:
            self.write_log("SCREENSHOT_ERROR", f"Screenshot capture failed: {e}")
            
        return None
    
    def get_status(self) -> Dict[str, Any]:
        """Get enhanced daemon status"""
        base_status = super().get_status()
        base_status.update({
            'browser_connected': self.browser_connected,
            'target_url': self.target_url,
            'connection_attempts': self.connection_attempts,
            'logs_captured': self.log_count,
            'ports': self.ports,
            'devtools_connected': self.devtools_monitor.connected if self.devtools_monitor else False
        })
        return base_status
    
    async def run(self):
        """Main daemon execution loop"""
        self.write_log("RUN_START", "DevTools daemon starting main loop")
        
        # Initial connection attempt
        await self.attempt_connection()
        
        # Start connection monitoring
        await self.monitor_connection()
        
        # Cleanup
        if self.devtools_monitor:
            await self.devtools_monitor.disconnect()
            self.write_log("CLEANUP", "DevTools monitor disconnected")


# Convenience function for Portal integration
async def start_devtools_daemon(target_url: str = "localhost:9000", ports: list = None) -> str:
    """Start a DevTools daemon and return its ID"""
    from ..core.daemon_manager import daemon_manager
    
    daemon = DevToolsDaemon(target_url, ports)
    daemon_id = await daemon_manager.start_daemon(daemon)
    
    return daemon_id