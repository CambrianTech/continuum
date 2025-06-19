#!/usr/bin/env python3
"""
Continuous DevTools Demo - PROOF OF PERSISTENCE
Takes screenshots and captures logs every 10 seconds to prove system works
"""

import subprocess
import time
import sys
import asyncio
import json
from pathlib import Path
from datetime import datetime

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent / "python-client"))

class ContinuousDevToolsDemo:
    def __init__(self):
        self.opera_process = None
        self.monitor_process = None
        self.screenshot_count = 0
        self.log_count = 0
        self.start_time = datetime.now()
        self.screenshot_dir = Path(".continuum/screenshots")
        self.logs_dir = Path(".continuum/demo_logs")
        
        # Create directories
        self.screenshot_dir.mkdir(parents=True, exist_ok=True)
        self.logs_dir.mkdir(parents=True, exist_ok=True)
    
    def run_command_quiet(self, cmd):
        """Run command without output spam"""
        try:
            if isinstance(cmd, str):
                result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=15)
            else:
                result = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
            return result.returncode == 0, result.stdout, result.stderr
        except Exception as e:
            return False, "", str(e)
    
    def setup_devtools_system(self):
        """Set up the complete DevTools system"""
        print("🎯 CONTINUOUS DEVTOOLS DEMO")
        print("=" * 50)
        
        # Kill existing Opera
        print("🔧 Cleaning up existing Opera processes...")
        self.run_command_quiet(['pkill', '-f', 'Opera.*remote-debugging-port'])
        time.sleep(2)
        
        # Launch Opera in debug mode
        print("🚀 Launching Opera in debug mode...")
        opera_cmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            '--remote-debugging-port=9222',
            '--disable-web-security',
            '--user-data-dir=/tmp/opera-devtools-continuous',
            'http://localhost:9000'
        ]
        
        try:
            self.opera_process = subprocess.Popen(
                opera_cmd, 
                stdout=subprocess.DEVNULL, 
                stderr=subprocess.DEVNULL
            )
            print(f"✅ Opera launched (PID: {self.opera_process.pid})")
            time.sleep(5)
        except Exception as e:
            print(f"❌ Failed to launch Opera: {e}")
            return False
        
        # Test DevTools connection
        print("🔌 Testing DevTools connection...")
        success, output, error = self.run_command_quiet(['curl', '-s', 'http://localhost:9222/json'])
        if success and 'devtoolsFrontendUrl' in output:
            print("✅ DevTools port 9222 responding")
        else:
            print("❌ DevTools port not responding")
            return False
        
        print("\n🎉 SYSTEM READY - Starting continuous monitoring...")
        return True
    
    def take_screenshot(self):
        """Take a timestamped screenshot"""
        timestamp = datetime.now().strftime("%H%M%S")
        filename = f"continuous_demo_{timestamp}"
        
        success, output, error = self.run_command_quiet([
            sys.executable, 'python-client/take_devtools_screenshot.py', filename
        ])
        
        if success:
            self.screenshot_count += 1
            print(f"📸 Screenshot #{self.screenshot_count}: {filename}.png")
            return True
        else:
            print(f"❌ Screenshot failed: {error[:50]}...")
            return False
    
    def capture_logs(self):
        """Capture current logs to file"""
        timestamp = datetime.now().strftime("%H%M%S")
        log_file = self.logs_dir / f"logs_{timestamp}.json"
        
        # Get portal logs
        success, output, error = self.run_command_quiet([
            sys.executable, 'python-client/ai-portal.py', '--logs', '5'
        ])
        
        if success:
            # Save logs to file
            log_data = {
                'timestamp': datetime.now().isoformat(),
                'portal_logs': output,
                'screenshot_count': self.screenshot_count,
                'uptime_seconds': (datetime.now() - self.start_time).total_seconds()
            }
            
            with open(log_file, 'w') as f:
                json.dump(log_data, f, indent=2)
            
            self.log_count += 1
            print(f"📋 Logs #{self.log_count}: {log_file.name}")
            return True
        else:
            print(f"❌ Log capture failed: {error[:50]}...")
            return False
    
    def check_system_health(self):
        """Check if Opera and DevTools are still working"""
        # Check if Opera is still running
        if self.opera_process and self.opera_process.poll() is not None:
            print("⚠️ Opera process died")
            return False
        
        # Check DevTools port
        success, output, error = self.run_command_quiet(['curl', '-s', 'http://localhost:9222/json'])
        if not success:
            print("⚠️ DevTools port not responding")
            return False
        
        return True
    
    def show_status(self):
        """Show current system status"""
        uptime = datetime.now() - self.start_time
        print(f"⏱️ Uptime: {uptime.total_seconds():.0f}s | Screenshots: {self.screenshot_count} | Logs: {self.log_count}")
    
    def run_continuous_monitoring(self):
        """Run the continuous monitoring loop"""
        print("🔄 STARTING CONTINUOUS MONITORING")
        print("📸 Taking screenshot + logs every 10 seconds")
        print("⌨️ Press Ctrl+C to stop")
        print("-" * 50)
        
        cycle = 0
        
        try:
            while True:
                cycle += 1
                print(f"\n🔄 Cycle #{cycle} - {datetime.now().strftime('%H:%M:%S')}")
                
                # Check system health
                if not self.check_system_health():
                    print("❌ System health check failed - stopping")
                    break
                
                # Take screenshot
                screenshot_success = self.take_screenshot()
                
                # Capture logs  
                logs_success = self.capture_logs()
                
                # Show status
                self.show_status()
                
                if not (screenshot_success and logs_success):
                    print("⚠️ Some operations failed")
                
                # Wait 10 seconds
                print("⏳ Waiting 10 seconds...")
                time.sleep(10)
                
        except KeyboardInterrupt:
            print("\n🛑 Monitoring stopped by user")
        except Exception as e:
            print(f"\n❌ Error during monitoring: {e}")
        finally:
            self.cleanup()
    
    def cleanup(self):
        """Clean up processes"""
        print("\n🧹 CLEANUP")
        
        if self.opera_process and self.opera_process.poll() is None:
            print("🔧 Terminating Opera...")
            self.opera_process.terminate()
            try:
                self.opera_process.wait(timeout=5)
                print("✅ Opera terminated")
            except subprocess.TimeoutExpired:
                self.opera_process.kill()
                print("⚠️ Opera killed forcefully")
        
        print(f"\n📊 FINAL STATS:")
        print(f"   Screenshots taken: {self.screenshot_count}")
        print(f"   Log captures: {self.log_count}")
        print(f"   Total uptime: {(datetime.now() - self.start_time).total_seconds():.0f} seconds")
        print(f"   Screenshots dir: {self.screenshot_dir}")
        print(f"   Logs dir: {self.logs_dir}")

def main():
    demo = ContinuousDevToolsDemo()
    
    if demo.setup_devtools_system():
        demo.run_continuous_monitoring()
    else:
        print("❌ Failed to set up DevTools system")
        return 1
    
    return 0

if __name__ == "__main__":
    sys.exit(main())