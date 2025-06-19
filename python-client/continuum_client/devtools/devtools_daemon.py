"""
DevTools Daemon - Object-Oriented DevTools Monitoring
Inherits from BaseDaemon for proper OOP structure
"""

import asyncio
from typing import Dict, Any, Optional
from pathlib import Path
from ..core.daemon_manager import BaseDaemon
from .log_monitor import DevToolsLogMonitor


class DevToolsDaemon(BaseDaemon):
    """Object-oriented DevTools monitoring daemon"""
    
    def __init__(self, target_url: str = "localhost:9000", ports: list = None, 
                 screenshot_dir: str = None, log_dir: str = None):
        super().__init__("devtools")
        
        self.target_url = target_url
        self.ports = ports or [9222, 9223]
        self.devtools_monitor = None
        self.browser_connected = False
        self.connection_attempts = 0
        self.log_count = 0
        
        # Configure directories
        self.screenshot_dir = self._setup_screenshot_dir(screenshot_dir)
        self.log_dir = log_dir  # Custom log dir (uses default daemon logs if None)
        
    def _setup_screenshot_dir(self, custom_dir: Optional[str]) -> Path:
        """Setup screenshot directory - use custom, Continuum default, or daemon default"""
        
        if custom_dir:
            # Use provided custom directory
            screenshot_dir = Path(custom_dir)
        else:
            # Look for Continuum screenshot directory first
            continuum_base = Path.cwd()
            while continuum_base != continuum_base.parent:
                if (continuum_base / '.continuum').exists():
                    break
                continuum_base = continuum_base.parent
            
            # Try Continuum's screenshot directory
            continuum_screenshots = continuum_base / '.continuum' / 'screenshots'
            if continuum_screenshots.exists():
                screenshot_dir = continuum_screenshots
                self.write_log("SCREENSHOT_DIR", f"Using Continuum screenshot directory: {screenshot_dir}")
            else:
                # Fall back to daemon-specific directory
                screenshot_dir = continuum_base / '.continuum' / 'daemons' / 'devtools' / 'screenshots'
                self.write_log("SCREENSHOT_DIR", f"Using daemon screenshot directory: {screenshot_dir}")
        
        # Ensure directory exists
        screenshot_dir.mkdir(parents=True, exist_ok=True)
        return screenshot_dir
        
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
        """Attempt to connect to DevTools on available ports with healing"""
        self.connection_attempts += 1
        
        self.write_log("CONNECTION_ATTEMPT", f"Attempting DevTools connection (attempt {self.connection_attempts})")
        
        for port in self.ports:
            try:
                self.write_log("TRYING_PORT", f"Trying DevTools port {port}")
                
                # If connection fails multiple times, try healing the port
                if self.connection_attempts > 2:
                    await self._heal_devtools_port(port)
                
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
                    
                    # Register as screenshot service with Continuum
                    await self._register_screenshot_service()
                    
                    return True
                    
            except Exception as e:
                self.write_log("CONNECTION_ERROR", f"Port {port} failed: {e}", {
                    'port': port,
                    'error': str(e)
                })
                continue
        
        self.write_log("CONNECTION_FAILED", f"All ports failed on attempt {self.connection_attempts}")
        return False
    
    async def _heal_devtools_port(self, port: int):
        """Heal DevTools port by killing existing connections and restarting browser"""
        try:
            self.write_log("HEALING_START", f"Healing DevTools port {port}")
            
            import subprocess
            import signal
            import time
            
            # Kill existing connections on the port
            try:
                result = subprocess.run(['lsof', '-ti', f':{port}'], 
                                      capture_output=True, text=True, timeout=5)
                if result.stdout.strip():
                    pids = result.stdout.strip().split('\n')
                    for pid in pids:
                        if pid:
                            try:
                                subprocess.run(['kill', '-9', pid], timeout=2)
                                self.write_log("HEALING_KILL", f"Killed process {pid} on port {port}")
                            except:
                                pass
            except:
                pass
            
            # Kill Opera processes if needed
            try:
                subprocess.run(['pkill', '-f', 'Opera.*remote-debugging-port'], timeout=5)
                self.write_log("HEALING_KILL_OPERA", "Killed Opera processes with remote debugging")
                await asyncio.sleep(2)  # Wait for cleanup
            except:
                pass
            
            # Restart Opera with DevTools
            try:
                opera_cmd = [
                    '/Applications/Opera GX.app/Contents/MacOS/Opera',
                    f'--remote-debugging-port={port}',
                    '--disable-web-security',
                    f'--user-data-dir=/tmp/opera-devtools-{port}',
                    'http://localhost:9000'
                ]
                
                subprocess.Popen(opera_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                self.write_log("HEALING_RESTART", f"Restarted Opera with DevTools on port {port}")
                
                # Wait for Opera to start
                await asyncio.sleep(5)
                
            except Exception as e:
                self.write_log("HEALING_ERROR", f"Failed to restart Opera: {e}")
                
        except Exception as e:
            self.write_log("HEALING_FAILED", f"Port healing failed: {e}")
    
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
    
    async def capture_screenshot(self, filename: str = None, format: str = "png", 
                               quality: int = 90) -> Optional[str]:
        """Capture screenshot via DevTools Protocol and save to configured directory"""
        if not self.devtools_monitor or not self.devtools_monitor.connected:
            self.write_log("SCREENSHOT_ERROR", "Cannot capture screenshot - DevTools not connected")
            return None
            
        try:
            self.write_log("SCREENSHOT_START", "Capturing screenshot via DevTools Protocol")
            
            # Use DevTools Protocol to capture screenshot
            params = {"format": format}
            if format == "jpeg":
                params["quality"] = quality
                
            screenshot_data = await self.devtools_monitor._send_command("Page.captureScreenshot", params)
            
            if screenshot_data and 'result' in screenshot_data:
                screenshot_b64 = screenshot_data['result'].get('data')
                
                # Generate filename if not provided
                if not filename:
                    from datetime import datetime
                    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
                    filename = f"continuum_screenshot_{timestamp}.{format}"
                elif not filename.endswith(f'.{format}'):
                    filename = f"{filename}.{format}"
                
                # Save to configured screenshot directory
                screenshot_path = self.screenshot_dir / filename
                
                import base64
                screenshot_bytes = base64.b64decode(screenshot_b64)
                with open(screenshot_path, 'wb') as f:
                    f.write(screenshot_bytes)
                
                self.write_log("SCREENSHOT_SUCCESS", f"Screenshot saved: {screenshot_path}", {
                    'filename': filename,
                    'path': str(screenshot_path),
                    'format': format,
                    'quality': quality if format == "jpeg" else None,
                    'size_bytes': len(screenshot_bytes)
                })
                
                return str(screenshot_path)
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
async def start_devtools_daemon(target_url: str = "localhost:9000", ports: list = None,
                              screenshot_dir: str = None, log_dir: str = None) -> str:
    """Start a DevTools daemon and return its ID"""
    from ..core.daemon_manager import daemon_manager
    
    daemon = DevToolsDaemon(target_url, ports, screenshot_dir, log_dir)
    daemon_id = await daemon_manager.start_daemon(daemon)
    
    return daemon_id