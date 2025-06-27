#!/usr/bin/env python3
"""
Continuum Integrated System Launcher
Bridges TypeScript daemon system with Python DevTools monitoring
Called by ai-portal.py --devtools to start the complete system
"""

import asyncio
import subprocess
import sys
import time
import signal
from pathlib import Path
from typing import Optional

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client.devtools.devtools_daemon import DevToolsDaemon
from continuum_client.core.daemon_manager import daemon_manager

class IntegratedContinuumSystem:
    """Complete Continuum system with TypeScript daemons + Python DevTools"""
    
    def __init__(self):
        self.typescript_process: Optional[subprocess.Popen] = None
        self.devtools_daemon: Optional[DevToolsDaemon] = None
        self.browser_process: Optional[subprocess.Popen] = None
        self.running = False
        
    async def start(self):
        """Start the complete integrated system"""
        print("🚀 STARTING INTEGRATED CONTINUUM SYSTEM")
        print("=" * 60)
        
        try:
            # Step 1: Kill any existing processes
            await self.cleanup_existing_processes()
            
            # Step 2: Start TypeScript daemon system
            print("📡 Starting TypeScript daemon system...")
            await self.start_typescript_system()
            
            # Step 3: Launch browser in debug mode
            print("🌐 Launching browser in debug mode...")
            await self.launch_debug_browser()
            
            # Step 4: Start Python DevTools monitoring
            print("🔌 Starting DevTools monitoring...")
            await self.start_devtools_monitoring()
            
            # Step 5: Register DevTools daemon
            print("📋 Registering DevTools daemon...")
            await self.register_devtools_daemon()
            
            # Step 6: Setup self-healing
            print("🔧 Setting up self-healing...")
            self.setup_self_healing()
            
            self.running = True
            print("\n✅ INTEGRATED CONTINUUM SYSTEM READY")
            print("   🌐 Browser UI: http://localhost:9000")
            print("   📡 WebSocket: ws://localhost:9000") 
            print("   🎨 TypeScript Daemons: Active")
            print("   🔌 DevTools Monitoring: Active")
            print("   📸 Screenshot Service: Available")
            print("   🔧 Self-healing: Active")
            
        except Exception as e:
            print(f"❌ Failed to start integrated system: {e}")
            await self.stop()
            raise
            
    async def stop(self):
        """Stop all system components"""
        print("🛑 Stopping integrated system...")
        
        try:
            # Stop DevTools daemon
            if self.devtools_daemon:
                await self.devtools_daemon.stop()
                print("✅ DevTools daemon stopped")
                
            # Stop browser process
            if self.browser_process:
                self.browser_process.terminate()
                try:
                    self.browser_process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.browser_process.kill()
                print("✅ Browser process stopped")
                
            # Stop TypeScript system
            if self.typescript_process:
                self.typescript_process.terminate()
                try:
                    self.typescript_process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    self.typescript_process.kill()
                print("✅ TypeScript daemon system stopped")
                
            self.running = False
            print("✅ Integrated system stopped")
            
        except Exception as e:
            print(f"⚠️ Error during shutdown: {e}")
            
    async def cleanup_existing_processes(self):
        """Kill any existing Continuum or browser processes"""
        print("🧹 Cleaning up existing processes...")
        
        # Kill existing Opera debug processes
        try:
            subprocess.run(['pkill', '-f', 'Opera.*remote-debugging-port'], 
                         timeout=5, capture_output=True)
        except:
            pass
            
        # Kill existing TypeScript daemons
        try:
            subprocess.run(['pkill', '-f', 'WebSocketDaemon.ts'], 
                         timeout=5, capture_output=True)
            subprocess.run(['pkill', '-f', 'RendererDaemon.ts'], 
                         timeout=5, capture_output=True)
            subprocess.run(['pkill', '-f', 'BrowserManagerDaemon.ts'], 
                         timeout=5, capture_output=True)
        except:
            pass
            
        # Wait for cleanup
        await asyncio.sleep(2)
        print("✅ Cleanup completed")
        
    async def start_typescript_system(self):
        """Start the TypeScript daemon system"""
        script_path = Path(__file__).parent.parent / "start-continuum-system.ts"
        
        if not script_path.exists():
            raise FileNotFoundError(f"TypeScript system launcher not found: {script_path}")
            
        # Start TypeScript system
        self.typescript_process = subprocess.Popen([
            'npx', 'tsx', str(script_path)
        ], cwd=script_path.parent, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        
        # Wait for system to be ready
        print("⏳ Waiting for TypeScript daemons to start...")
        for _ in range(20):  # Wait up to 20 seconds
            try:
                # Check if WebSocket server is responding
                result = subprocess.run([
                    'curl', '-s', '--connect-timeout', '1', 
                    'http://localhost:9000'
                ], capture_output=True)
                
                if result.returncode == 0:
                    print("✅ TypeScript daemon system ready")
                    return
                    
            except:
                pass
                
            await asyncio.sleep(1)
            
        # Check if process is still running
        if self.typescript_process.poll() is not None:
            stdout, stderr = self.typescript_process.communicate()
            print(f"❌ TypeScript system failed to start:")
            print(f"STDOUT: {stdout.decode()}")
            print(f"STDERR: {stderr.decode()}")
            raise RuntimeError("TypeScript daemon system failed to start")
            
        print("✅ TypeScript daemon system started (may still be initializing)")
        
    async def launch_debug_browser(self):
        """Launch browser in debug mode for DevTools"""
        browser_cmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            '--remote-debugging-port=9222',
            '--disable-web-security',
            '--user-data-dir=/tmp/continuum-opera-debug',
            '--new-window',
            'http://localhost:9000'
        ]
        
        try:
            self.browser_process = subprocess.Popen(
                browser_cmd, 
                stdout=subprocess.DEVNULL, 
                stderr=subprocess.DEVNULL
            )
            
            # Wait for DevTools port to be ready
            print("⏳ Waiting for DevTools port...")
            for _ in range(15):
                try:
                    result = subprocess.run([
                        'curl', '-s', '--connect-timeout', '1',
                        'http://localhost:9222/json'
                    ], capture_output=True)
                    
                    if result.returncode == 0 and b'devtoolsFrontendUrl' in result.stdout:
                        print("✅ Browser launched with DevTools on port 9222")
                        return
                        
                except:
                    pass
                    
                await asyncio.sleep(1)
                
            print("⚠️ DevTools port not responding, but browser should be running")
            
        except Exception as e:
            print(f"⚠️ Failed to launch browser: {e}")
            print("   Manual navigation to http://localhost:9000 required")
            
    async def start_devtools_monitoring(self):
        """Start Python DevTools monitoring daemon"""
        self.devtools_daemon = DevToolsDaemon(
            target_url="localhost:9000",
            ports=[9222, 9223],
            screenshot_dir=str(Path.cwd() / '.continuum' / 'screenshots')
        )
        
        # Start the daemon
        await self.devtools_daemon.start()
        
        # Attempt connection
        connected = await self.devtools_daemon.attempt_connection()
        if connected:
            print("✅ DevTools monitoring connected")
        else:
            print("⚠️ DevTools monitoring started but not connected")
            
    async def register_devtools_daemon(self):
        """Register DevTools daemon with daemon manager"""
        if self.devtools_daemon:
            daemon_manager.register_daemon(self.devtools_daemon)
            
            # Register screenshot service
            daemon_manager.register_screenshot_service({
                'name': 'devtools-screenshots',
                'daemon_id': self.devtools_daemon.daemon_id,
                'type': 'devtools',
                'status': 'active',
                'screenshot_dir': str(self.devtools_daemon.screenshot_dir)
            })
            
            print("✅ DevTools daemon registered with screenshot service")
            
    def setup_self_healing(self):
        """Setup self-healing mechanisms"""
        # Create periodic health check task
        async def health_check():
            while self.running:
                try:
                    # Check TypeScript system health
                    if self.typescript_process and self.typescript_process.poll() is not None:
                        print("🔧 Self-healing: TypeScript system crashed, restarting...")
                        await self.start_typescript_system()
                        
                    # Check DevTools connection
                    if self.devtools_daemon and not await self.devtools_daemon.attempt_connection():
                        print("🔧 Self-healing: DevTools disconnected, reconnecting...")
                        await self.devtools_daemon.attempt_connection()
                        
                except Exception as e:
                    print(f"🔧 Self-healing error: {e}")
                    
                await asyncio.sleep(30)  # Check every 30 seconds
                
        # Start health check task
        asyncio.create_task(health_check())
        print("✅ Self-healing monitoring active")
        
    async def get_system_status(self):
        """Get complete system status"""
        return {
            'running': self.running,
            'typescript_system': 'running' if (self.typescript_process and self.typescript_process.poll() is None) else 'stopped',
            'browser': 'running' if (self.browser_process and self.browser_process.poll() is None) else 'stopped',
            'devtools': self.devtools_daemon.get_status() if self.devtools_daemon else None,
            'screenshot_services': daemon_manager.find_screenshot_services()
        }

async def main():
    """Main entry point"""
    system = IntegratedContinuumSystem()
    
    # Setup signal handlers for graceful shutdown
    def signal_handler(sig, frame):
        print(f"\n🛑 Received signal {sig}, shutting down...")
        asyncio.create_task(system.stop())
        
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    try:
        await system.start()
        
        # Keep running
        print("\n⏰ System running... (Press Ctrl+C to stop)")
        while system.running:
            await asyncio.sleep(1)
            
    except KeyboardInterrupt:
        print("\n🛑 Shutdown requested...")
    except Exception as e:
        print(f"💥 System error: {e}")
    finally:
        await system.stop()

if __name__ == "__main__":
    asyncio.run(main())