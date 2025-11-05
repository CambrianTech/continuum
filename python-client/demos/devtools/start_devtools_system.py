#!/usr/bin/env python3
"""
Continuum DevTools System - End-to-End Automation
Launches Opera in debug mode, starts persistent monitoring, enables screenshots
"""

import asyncio
import subprocess
import sys
import time
from pathlib import Path

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent / "python-client"))

from continuum_client.devtools.devtools_daemon import start_devtools_daemon
from continuum_client.core.daemon_manager import daemon_manager

class DevToolsSystem:
    """Complete DevTools system automation"""
    
    def __init__(self):
        self.opera_process = None
        self.daemon_id = None
        self.monitoring = False
        
    def kill_existing_opera(self):
        """Kill any existing Opera processes with remote debugging"""
        print("🔧 Cleaning up existing Opera processes...")
        try:
            subprocess.run(['pkill', '-f', 'Opera.*remote-debugging-port'], 
                         timeout=5, capture_output=True)
            time.sleep(2)
            print("✅ Existing Opera processes terminated")
        except:
            print("ℹ️ No existing Opera processes found")
    
    def launch_opera_debug(self):
        """Launch Opera in debug mode to localhost:9000"""
        print("🚀 Launching Opera GX in debug mode...")
        
        opera_cmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            '--remote-debugging-port=9222',
            '--disable-web-security',
            '--user-data-dir=/tmp/opera-devtools-9222',
            'http://localhost:9000'
        ]
        
        try:
            self.opera_process = subprocess.Popen(
                opera_cmd, 
                stdout=subprocess.DEVNULL, 
                stderr=subprocess.DEVNULL
            )
            print(f"✅ Opera launched (PID: {self.opera_process.pid})")
            print("📍 Browser URL: http://localhost:9000")
            print("🔌 DevTools Port: 9222")
            time.sleep(5)  # Wait for Opera to start
            return True
        except Exception as e:
            print(f"❌ Failed to launch Opera: {e}")
            return False
    
    async def start_persistent_monitoring(self):
        """Start persistent DevTools daemon for logs and screenshots"""
        print("🔌 Starting persistent DevTools monitoring...")
        
        try:
            self.daemon_id = await start_devtools_daemon(
                target_url="localhost:9000",
                ports=[9222, 9223],
                screenshot_dir=None,  # Use default Continuum directory
                log_dir=None
            )
            
            daemon = daemon_manager.active_daemons.get(self.daemon_id)
            if daemon and daemon.browser_connected:
                print(f"✅ DevTools daemon started: {self.daemon_id}")
                print("📡 Real-time browser console logging active")
                print("📸 Screenshot capability enabled")
                print("🔍 Monitoring both client and server logs")
                self.monitoring = True
                return True
            else:
                print("❌ DevTools daemon failed to connect")
                return False
                
        except Exception as e:
            print(f"❌ Failed to start DevTools monitoring: {e}")
            return False
    
    async def take_test_screenshot(self, filename="devtools_system_test"):
        """Take a test screenshot to verify system is working"""
        if not self.monitoring:
            print("❌ DevTools monitoring not active")
            return False
            
        print(f"📸 Taking test screenshot: {filename}")
        
        try:
            daemon = daemon_manager.active_daemons.get(self.daemon_id)
            if daemon and daemon.browser_connected:
                screenshot_path = await daemon.capture_screenshot(filename)
                if screenshot_path:
                    print(f"✅ Screenshot saved: {screenshot_path}")
                    return screenshot_path
                else:
                    print("❌ Screenshot capture failed")
                    return False
            else:
                print("❌ DevTools daemon not connected")
                return False
                
        except Exception as e:
            print(f"❌ Screenshot error: {e}")
            return False
    
    def show_status(self):
        """Show current system status"""
        print("\n📊 DEVTOOLS SYSTEM STATUS:")
        print(f"🖥️ Opera Process: {'Running' if self.opera_process and self.opera_process.poll() is None else 'Stopped'}")
        print(f"🔌 DevTools Daemon: {'Active' if self.monitoring else 'Inactive'}")
        print(f"📋 Daemon ID: {self.daemon_id or 'None'}")
        
        if self.daemon_id:
            daemon = daemon_manager.active_daemons.get(self.daemon_id)
            if daemon:
                status = daemon.get_status()
                print(f"🌐 Browser Connected: {status.get('browser_connected', False)}")
                print(f"📊 Logs Captured: {status.get('logs_captured', 0)}")
                print(f"📸 Screenshot Dir: {daemon.screenshot_dir}")
    
    async def run_continuous_monitoring(self):
        """Run continuous monitoring until interrupted"""
        print("\n🔄 CONTINUOUS MONITORING ACTIVE")
        print("📡 DevTools logs streaming in real-time")
        print("📸 Screenshots available on demand")
        print("⌨️ Press Ctrl+C to stop")
        
        try:
            while True:
                await asyncio.sleep(1)
                
                # Health check every 30 seconds
                if int(time.time()) % 30 == 0:
                    daemon = daemon_manager.active_daemons.get(self.daemon_id)
                    if daemon and hasattr(daemon, 'log_count'):
                        print(f"💗 Health check: {daemon.log_count} browser logs captured")
                        
        except KeyboardInterrupt:
            print("\n🛑 Stopping DevTools system...")
            await self.cleanup()
    
    async def cleanup(self):
        """Clean up resources"""
        if self.daemon_id:
            daemon_manager.stop_daemon(self.daemon_id)
            print("✅ DevTools daemon stopped")
        
        if self.opera_process and self.opera_process.poll() is None:
            self.opera_process.terminate()
            try:
                self.opera_process.wait(timeout=5)
                print("✅ Opera process terminated")
            except subprocess.TimeoutExpired:
                self.opera_process.kill()
                print("⚠️ Opera process killed")

async def main():
    """Main execution function"""
    print("🎯 CONTINUUM DEVTOOLS SYSTEM STARTUP")
    print("=" * 50)
    
    system = DevToolsSystem()
    
    try:
        # Step 1: Clean up existing processes
        system.kill_existing_opera()
        
        # Step 2: Launch Opera in debug mode
        if not system.launch_opera_debug():
            return 1
        
        # Step 3: Start persistent monitoring
        if not await system.start_persistent_monitoring():
            return 1
        
        # Step 4: Take test screenshot
        screenshot_path = await system.take_test_screenshot()
        
        # Step 5: Show status
        system.show_status()
        
        print("\n🎉 DEVTOOLS SYSTEM FULLY OPERATIONAL")
        print("=" * 50)
        print("✅ Opera running in debug mode")
        print("✅ DevTools Protocol connected")
        print("✅ Real-time logging active")
        print("✅ Screenshot capability ready")
        
        if screenshot_path:
            print(f"✅ Test screenshot: {screenshot_path}")
        
        print("\nCommands available:")
        print("📸 Take screenshot: python take_devtools_screenshot.py <filename>")
        print("📋 View logs: python ai-portal.py --logs 5")
        print("📊 Check status: python ai-portal.py --daemons")
        
        # Step 6: Run continuous monitoring
        await system.run_continuous_monitoring()
        
    except KeyboardInterrupt:
        print("\n🛑 System interrupted by user")
        await system.cleanup()
        return 0
    except Exception as e:
        print(f"\n❌ System error: {e}")
        await system.cleanup()
        return 1

if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)