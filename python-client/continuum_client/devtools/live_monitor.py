"""
Live DevTools Monitor
Intelligent real-time browser monitoring with memory-based responsiveness
Built into the modular continuum_client.devtools platform
"""

import asyncio
import sys
import signal
import os
from pathlib import Path
from datetime import datetime
from collections import deque
import json
from typing import Dict, List, Callable, Optional, Any

from .log_monitor import DevToolsLogMonitor
from .browser_adapters import auto_detect_browsers

class LiveDevToolsMonitor:
    """
    Intelligent real-time DevTools monitor
    - Memory-based for responsiveness
    - Supports screenshots via DevTools Protocol
    - Modular and extensible
    """
    
    def __init__(self, max_memory_logs=100, max_recent_logs=20):
        self.running = True
        self.devtools_monitor = None
        
        # Memory-based log management for responsiveness
        self.recent_logs = deque(maxlen=max_recent_logs)
        self.memory_buffer = deque(maxlen=max_memory_logs)
        
        # Daemon-specific logging
        self.daemon_id = f"devtools-{datetime.now().strftime('%H%M%S')}"
        self.log_file = self.setup_daemon_logging()
        
        # Current activity tracking
        self.current_activity = {
            'browser_connected': False,
            'last_browser_activity': None,
            'last_command': None,
            'websocket_messages': 0,
            'screenshot_count': 0,
            'error_count': 0
        }
        
        # Live callbacks for immediate feedback
        self.live_callbacks = []
        
    def setup_daemon_logging(self) -> Path:
        """Setup daemon-specific logging"""
        # Create daemon logs directory
        daemon_logs_dir = Path.cwd()
        while daemon_logs_dir != daemon_logs_dir.parent:
            if (daemon_logs_dir / '.continuum').exists():
                break
            daemon_logs_dir = daemon_logs_dir.parent
        
        daemon_logs_dir = daemon_logs_dir / '.continuum' / 'daemons' / 'devtools'
        daemon_logs_dir.mkdir(parents=True, exist_ok=True)
        
        log_file = daemon_logs_dir / f"{self.daemon_id}.log"
        
        # Write initial log entry
        self.write_daemon_log("DAEMON_START", f"DevTools daemon {self.daemon_id} started")
        
        return log_file
        
    def write_daemon_log(self, level: str, message: str):
        """Write to daemon-specific log file"""
        try:
            timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]
            log_entry = f"[{timestamp}] [{level}] {message}\n"
            
            with open(self.log_file, 'a') as f:
                f.write(log_entry)
                
        except Exception as e:
            print(f"🚨 Daemon logging error: {e}")
            
    def get_daemon_logs(self, lines: int = 50) -> List[str]:
        """Get recent daemon log entries"""
        try:
            if not self.log_file.exists():
                return []
                
            with open(self.log_file, 'r') as f:
                all_lines = f.readlines()
                return all_lines[-lines:] if len(all_lines) >= lines else all_lines
                
        except Exception as e:
            return [f"Error reading daemon logs: {e}"]
        
        # DevTools capabilities
        self.capabilities = {
            'console_monitoring': True,
            'screenshot_capture': True, 
            'network_monitoring': False,
            'performance_monitoring': False
        }
        
    def add_live_callback(self, callback: Callable):
        """Add callback for immediate log processing"""
        self.live_callbacks.append(callback)
        
    def process_log_entry(self, log_entry: Dict[str, Any]):
        """Process log with intelligent prioritization"""
        # Add to memory structures
        self.recent_logs.append(log_entry)
        self.memory_buffer.append(log_entry)
        
        # Log to daemon file
        self.write_daemon_log("BROWSER_LOG", f"{log_entry.get('level', 'INFO')}: {log_entry.get('text', '')}")
        
        # Update activity tracking
        self.update_activity_tracking(log_entry)
        
        # Immediate callbacks for live feedback
        for callback in self.live_callbacks:
            try:
                callback(log_entry)
            except Exception as e:
                print(f"⚠️ Live callback error: {e}")
                
    def update_activity_tracking(self, log_entry: Dict[str, Any]):
        """Track current system state"""
        timestamp = log_entry.get('timestamp', datetime.now().isoformat())
        text = log_entry.get('text', '')
        level = log_entry.get('level', 'info')
        
        # Track browser activity
        if 'browser' in log_entry.get('source', ''):
            self.current_activity['browser_connected'] = True
            self.current_activity['last_browser_activity'] = timestamp
            
            if 'WebSocket message received' in text:
                self.current_activity['websocket_messages'] += 1
        
        # Track screenshot activity
        if 'screenshot' in text.lower():
            self.current_activity['screenshot_count'] += 1
            
        # Track errors
        if level.upper() in ['ERROR', 'WARN']:
            self.current_activity['error_count'] += 1

    async def handle_browser_log(self, log_entry: Dict[str, Any]):
        """Handle browser console log from DevTools"""
        enhanced_entry = {
            'type': 'browser_log',
            'timestamp': log_entry['timestamp'],
            'level': log_entry['level'],
            'text': log_entry['text'],
            'source': 'browser',
            'raw_timestamp': log_entry.get('raw_timestamp', 0)
        }
        
        self.process_log_entry(enhanced_entry)
        
        # Immediate console output for responsiveness
        timestamp = enhanced_entry['timestamp']
        level = enhanced_entry['level'].upper()
        text = enhanced_entry['text']
        print(f"🌐 [{timestamp}] {level}: {text}")

    async def start_devtools_monitoring(self) -> bool:
        """Start DevTools monitoring with fallback ports"""
        print("🔌 Live Monitor: Starting DevTools monitoring...")
        
        # Try standard DevTools ports
        for port in [9222, 9223]:
            try:
                self.devtools_monitor = DevToolsLogMonitor(
                    chrome_port=port,
                    target_url="localhost:9000",
                    log_callback=self.handle_browser_log
                )
                
                success = await self.devtools_monitor.connect()
                if success:
                    print(f"🔌 Live Monitor: Connected on port {port}")
                    self.write_daemon_log("CONNECTION", f"DevTools connected on port {port}")
                    self.current_activity['browser_connected'] = True
                    return True
                    
            except Exception as e:
                print(f"🔌 Live Monitor: Port {port} failed: {e}")
        
        print("🔌 Live Monitor: No browser found - will retry")
        return False

    async def capture_screenshot(self) -> Optional[str]:
        """Capture screenshot via DevTools Protocol"""
        if not self.devtools_monitor or not self.devtools_monitor.connected:
            print("📸 Screenshot: DevTools not connected")
            return None
            
        try:
            # Use DevTools Protocol to capture screenshot
            screenshot_data = await self.devtools_monitor._send_command("Page.captureScreenshot", {
                "format": "png",
                "quality": 90
            })
            
            if screenshot_data and 'result' in screenshot_data:
                self.current_activity['screenshot_count'] += 1
                print(f"📸 Screenshot captured ({self.current_activity['screenshot_count']} total)")
                return screenshot_data['result'].get('data')
            
        except Exception as e:
            print(f"📸 Screenshot error: {e}")
            
        return None

    async def monitor_system(self):
        """Monitor system with intelligent reconnection"""
        while self.running:
            try:
                # Check DevTools connection
                if not self.devtools_monitor or not self.devtools_monitor.connected:
                    print("🔌 Live Monitor: Reconnecting...")
                    await self.start_devtools_monitoring()
                
                # Health check
                if self.current_activity['error_count'] > 10:
                    print("⚠️ Live Monitor: High error count - resetting")
                    self.current_activity['error_count'] = 0
                
                await asyncio.sleep(5)  # Fast check for responsiveness
                
            except Exception as e:
                print(f"🔌 Live Monitor: System error: {e}")
                await asyncio.sleep(10)

    def get_current_status(self) -> Dict[str, Any]:
        """Get current system status"""
        return {
            'browser_connected': self.current_activity['browser_connected'],
            'recent_logs_count': len(self.recent_logs),
            'memory_buffer_count': len(self.memory_buffer),
            'last_activity': self.current_activity.get('last_browser_activity'),
            'error_count': self.current_activity['error_count'],
            'websocket_activity': self.current_activity['websocket_messages'],
            'screenshots_taken': self.current_activity['screenshot_count'],
            'capabilities': self.capabilities
        }
        
    def get_recent_activity(self, lines: int = 10) -> List[str]:
        """Get recent activity for immediate feedback"""
        recent = list(self.recent_logs)[-lines:]
        return [
            f"[{log.get('timestamp', 'unknown')}] {log.get('level', 'INFO').upper()}: {log.get('text', '')}"
            for log in recent
        ]
        
    def get_live_feed(self) -> Dict[str, Any]:
        """Get live feed of current activity"""
        return {
            'status': self.get_current_status(),
            'recent_activity': self.get_recent_activity(5),
            'timestamp': datetime.now().isoformat()
        }

    def setup_signal_handlers(self):
        """Setup signal handlers for clean shutdown"""
        def signal_handler(signum, frame):
            print(f"\n🔌 Live Monitor: Shutting down...")
            self.running = False

        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)

    async def run(self):
        """Main monitor loop"""
        print("🔌 Live DevTools Monitor: Starting intelligent real-time system...")
        
        self.setup_signal_handlers()
        
        # Add immediate feedback callback
        def immediate_feedback(log_entry):
            if log_entry.get('level', '').upper() == 'ERROR':
                print(f"🚨 IMMEDIATE: {log_entry.get('text', '')}")
            elif 'screenshot' in log_entry.get('text', '').lower():
                print(f"📸 CAPTURE: {log_entry.get('text', '')}")
                
        self.add_live_callback(immediate_feedback)
        
        try:
            # Start DevTools monitoring
            await self.start_devtools_monitoring()
            
            # Keep monitoring
            await self.monitor_system()
            
        finally:
            if self.devtools_monitor:
                await self.devtools_monitor.disconnect()
            print("🔌 Live Monitor: Stopped")

# Standalone execution capability
async def main():
    """Standalone execution"""
    import argparse
    parser = argparse.ArgumentParser(description='Live DevTools Monitor')
    parser.add_argument('--status', action='store_true', help='Show current status')
    parser.add_argument('--screenshot', action='store_true', help='Capture screenshot')
    args = parser.parse_args()
    
    monitor = LiveDevToolsMonitor()
    
    if args.status:
        print("📊 Live Monitor Status:")
        status = monitor.get_current_status()
        for key, value in status.items():
            print(f"  {key}: {value}")
        return
        
    if args.screenshot:
        await monitor.start_devtools_monitoring()
        screenshot = await monitor.capture_screenshot()
        if screenshot:
            print(f"📸 Screenshot captured (base64 length: {len(screenshot)})")
        else:
            print("📸 Screenshot failed")
        return
    
    # Run live monitoring
    await monitor.run()

if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\n🔌 Live Monitor: Interrupted by user")
    except Exception as e:
        print(f"🔌 Live Monitor: Fatal error: {e}")