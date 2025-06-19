#!/usr/bin/env python3
"""
Real-time DevTools Demo - LIVE LOG STREAMING
Shows logs as they come in, takes periodic screenshots, handles Opera welcome screen
"""

import subprocess
import time
import sys
import threading
import queue
from pathlib import Path
from datetime import datetime

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent / "python-client"))

class RealTimeDevToolsDemo:
    def __init__(self):
        self.opera_process = None
        self.monitor_process = None
        self.screenshot_count = 0
        self.start_time = datetime.now()
        self.screenshot_dir = Path(".continuum/screenshots")
        self.running = True
        self.log_queue = queue.Queue()
        
        # Create directories
        self.screenshot_dir.mkdir(parents=True, exist_ok=True)
    
    def setup_devtools_system(self):
        """Set up DevTools system with welcome screen handling"""
        print("🎯 REAL-TIME DEVTOOLS DEMO")
        print("=" * 50)
        
        # Kill existing Opera
        print("🔧 Cleaning up existing Opera processes...")
        subprocess.run(['pkill', '-f', 'Opera.*remote-debugging-port'], 
                      capture_output=True)
        time.sleep(2)
        
        # Launch Opera with welcome screen disabled and direct to localhost:9000
        print("🚀 Launching Opera in debug mode (bypassing welcome screen)...")
        opera_cmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            '--remote-debugging-port=9222',
            '--disable-web-security',
            '--disable-features=TranslateUI',
            '--disable-component-update', 
            '--disable-background-timer-throttling',
            '--disable-backgrounding-occluded-windows',
            '--disable-renderer-backgrounding',
            '--no-first-run',
            '--no-default-browser-check',
            '--user-data-dir=/tmp/opera-devtools-realtime',
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
            
            # Test DevTools connection
            result = subprocess.run(['curl', '-s', 'http://localhost:9222/json'], 
                                  capture_output=True, text=True, timeout=5)
            if result.returncode == 0 and 'devtoolsFrontendUrl' in result.stdout:
                print("✅ DevTools port 9222 responding")
                return True
            else:
                print("❌ DevTools port not responding")
                return False
                
        except Exception as e:
            print(f"❌ Failed to launch Opera: {e}")
            return False
    
    def start_devtools_monitoring(self):
        """Start DevTools monitoring in separate thread with live output"""
        print("🔌 Starting DevTools monitoring with live output...")
        
        try:
            # Start DevTools monitoring process
            self.monitor_process = subprocess.Popen([
                sys.executable, 'python-client/ai-portal.py', '--devtools'
            ], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, 
               text=True, bufsize=1, universal_newlines=True)
            
            # Start thread to read output in real-time
            monitor_thread = threading.Thread(
                target=self.read_monitor_output, 
                daemon=True
            )
            monitor_thread.start()
            
            print(f"✅ DevTools monitor started (PID: {self.monitor_process.pid})")
            time.sleep(3)  # Give it time to connect
            return True
            
        except Exception as e:
            print(f"❌ Failed to start DevTools monitor: {e}")
            return False
    
    def read_monitor_output(self):
        """Read DevTools monitor output in real-time"""
        while self.running and self.monitor_process:
            try:
                line = self.monitor_process.stdout.readline()
                if line:
                    timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
                    print(f"🔌 [{timestamp}] {line.strip()}")
                    
                    # Log to file as well
                    with open('.continuum/realtime_devtools.log', 'a') as f:
                        f.write(f"[{timestamp}] {line}")
                        f.flush()
                else:
                    if self.monitor_process.poll() is not None:
                        break
                    time.sleep(0.1)
            except Exception as e:
                print(f"⚠️ Monitor output error: {e}")
                break
    
    def take_periodic_screenshots(self):
        """Take screenshots every 15 seconds in background thread"""
        while self.running:
            try:
                time.sleep(15)  # Wait 15 seconds between screenshots
                
                if not self.running:
                    break
                    
                timestamp = datetime.now().strftime("%H%M%S")
                filename = f"realtime_demo_{timestamp}"
                
                result = subprocess.run([
                    sys.executable, 'python-client/take_devtools_screenshot.py', filename
                ], capture_output=True, text=True, timeout=30)
                
                if result.returncode == 0:
                    self.screenshot_count += 1
                    print(f"📸 [{datetime.now().strftime('%H:%M:%S')}] Screenshot #{self.screenshot_count}: {filename}.png")
                else:
                    print(f"❌ [{datetime.now().strftime('%H:%M:%S')}] Screenshot failed")
                    
            except Exception as e:
                print(f"⚠️ Screenshot error: {e}")
                time.sleep(5)  # Wait before retrying
    
    def show_periodic_status(self):
        """Show status every 30 seconds"""
        while self.running:
            try:
                time.sleep(30)
                if not self.running:
                    break
                    
                uptime = datetime.now() - self.start_time
                print(f"\n📊 [{datetime.now().strftime('%H:%M:%S')}] STATUS:")
                print(f"   ⏱️ Uptime: {uptime.total_seconds():.0f}s")
                print(f"   📸 Screenshots: {self.screenshot_count}")
                print(f"   🔌 Opera PID: {self.opera_process.pid if self.opera_process else 'None'}")
                print(f"   🔍 Monitor PID: {self.monitor_process.pid if self.monitor_process else 'None'}")
                print()
                
            except Exception as e:
                print(f"⚠️ Status error: {e}")
                time.sleep(10)
    
    def run_demonstration(self):
        """Run the complete real-time demonstration"""
        print("\n🔄 STARTING REAL-TIME DEMONSTRATION")
        print("📡 Live DevTools logs will stream below")
        print("📸 Screenshots every 15 seconds")
        print("📊 Status updates every 30 seconds")
        print("⌨️ Press Ctrl+C to stop")
        print("-" * 50)
        
        # Start background threads
        screenshot_thread = threading.Thread(target=self.take_periodic_screenshots, daemon=True)
        status_thread = threading.Thread(target=self.show_periodic_status, daemon=True)
        
        screenshot_thread.start()
        status_thread.start()
        
        try:
            # Just keep the main thread alive and show live output
            while self.running:
                time.sleep(1)
                
                # Check if Opera is still running
                if self.opera_process and self.opera_process.poll() is not None:
                    print("⚠️ Opera process ended")
                    break
                    
        except KeyboardInterrupt:
            print("\n🛑 Demonstration stopped by user")
        except Exception as e:
            print(f"\n❌ Error during demonstration: {e}")
        finally:
            self.cleanup()
    
    def cleanup(self):
        """Clean up all processes"""
        print("\n🧹 CLEANUP")
        self.running = False
        
        if self.monitor_process and self.monitor_process.poll() is None:
            print("🔧 Terminating DevTools monitor...")
            self.monitor_process.terminate()
            try:
                self.monitor_process.wait(timeout=5)
                print("✅ Monitor terminated")
            except subprocess.TimeoutExpired:
                self.monitor_process.kill()
                print("⚠️ Monitor killed forcefully")
        
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
        print(f"   Total uptime: {(datetime.now() - self.start_time).total_seconds():.0f} seconds")
        print(f"   Screenshots dir: {self.screenshot_dir}")
        print(f"   Live log file: .continuum/realtime_devtools.log")

def main():
    demo = RealTimeDevToolsDemo()
    
    if not demo.setup_devtools_system():
        print("❌ Failed to set up DevTools system")
        return 1
    
    if not demo.start_devtools_monitoring():
        print("❌ Failed to start DevTools monitoring")
        return 1
    
    print("✅ System ready - starting demonstration...")
    demo.run_demonstration()
    
    return 0

if __name__ == "__main__":
    sys.exit(main())