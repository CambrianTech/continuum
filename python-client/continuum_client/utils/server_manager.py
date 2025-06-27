"""
Continuum Server Management Utilities
Handles starting, stopping, and health checking of Continuum server
"""

import os
import sys
import subprocess
import time
import signal
import atexit
from pathlib import Path
from typing import Optional

try:
    import requests
except ImportError:
    requests = None

class ContinuumServerManager:
    """Manages Continuum server lifecycle for testing and development"""
    
    def __init__(self, port: Optional[int] = None):
        from .config import get_continuum_ws_port
        # Get port from environment or use default
        self.port = port or get_continuum_ws_port()
        self.process: Optional[subprocess.Popen] = None
        atexit.register(self.stop)
    
    def find_continuum_executable(self) -> Optional[Path]:
        """Find the Continuum clean entry point"""
        # Start from python-client and go up to find continuum (clean CLI)
        current_dir = Path(__file__).parent
        
        # Go up from continuum_client/utils/ to find continuum clean CLI
        while current_dir.name != '/' and len(current_dir.parts) > 1:
            continuum_path = current_dir / 'continuum'
            if continuum_path.exists():
                return continuum_path
            current_dir = current_dir.parent
        
        # Clean TypeScript entry point only!
        return None
    
    def kill_existing_server(self) -> bool:
        """Kill any existing server on this port"""
        try:
            # Find process using the port
            result = subprocess.run(
                ['lsof', '-ti', f':{self.port}'], 
                capture_output=True, 
                text=True
            )
            
            if result.returncode == 0 and result.stdout.strip():
                pids = result.stdout.strip().split('\n')
                print(f"🔫 Killing existing servers on port {self.port}: {pids}")
                
                for pid in pids:
                    try:
                        subprocess.run(['kill', '-9', pid], check=True)
                        print(f"💀 Killed process {pid}")
                    except subprocess.CalledProcessError:
                        print(f"⚠️ Failed to kill process {pid}")
                
                # Wait a moment for processes to die
                import time
                time.sleep(2)
                return True
            
            return False
            
        except Exception as e:
            print(f"⚠️ Error killing existing server: {e}")
            return False
    
    def is_server_healthy(self, timeout: int = 30) -> bool:
        """Check if Continuum server is running and healthy"""
        if not requests:
            print("⚠️ requests library not available, skipping health check")
            return True
            
        url = f"http://localhost:{self.port}"
        
        for _ in range(timeout):
            try:
                response = requests.get(url, timeout=2)
                if response.status_code == 200:
                    return True
            except requests.exceptions.RequestException:
                pass
            time.sleep(1)
        
        return False
    
    def is_websocket_server_healthy(self, timeout: int = 10) -> bool:
        """Check if TypeScript WebSocket server is running on the port"""
        import socket
        
        for _ in range(timeout):
            try:
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                sock.settimeout(1)
                result = sock.connect_ex(('localhost', self.port))
                sock.close()
                if result == 0:
                    print(f"✅ WebSocket server detected on port {self.port}")
                    return True
            except Exception as e:
                print(f"🔍 Port check failed: {e}")
            time.sleep(1)
        
        print(f"❌ No WebSocket server detected on port {self.port}")
        return False
    
    def wait_for_typescript_startup(self, timeout: int = 30) -> bool:
        """Monitor process output for TypeScript daemon startup confirmation"""
        if not self.process:
            return False
            
        import select
        startup_messages = [
            "✅ Core TypeScript daemons operational",
            "🌐 WebSocket server ready on port",
            "✅ WebSocket server ACTUALLY listening"
        ]
        
        start_time = time.time()
        output_buffer = ""
        
        while time.time() - start_time < timeout:
            if self.process.poll() is not None:
                print(f"❌ Process exited with code {self.process.returncode}")
                return False
                
            try:
                # Read available output
                ready, _, _ = select.select([self.process.stdout], [], [], 0.1)
                if ready:
                    chunk = self.process.stdout.read(1024).decode('utf-8', errors='ignore')
                    output_buffer += chunk
                    print(f"📝 Startup log: {chunk.strip()}")
                    
                    # Check for startup confirmation
                    for message in startup_messages:
                        if message in output_buffer:
                            print(f"✅ Detected startup confirmation: {message}")
                            return True
                            
            except Exception as e:
                print(f"⚠️ Error reading process output: {e}")
                
        print(f"❌ Startup timeout after {timeout}s")
        return False
    
    def show_process_output(self):
        """Show process output for debugging"""
        if not self.process:
            return
            
        try:
            stdout, stderr = self.process.communicate(timeout=2)
            if stdout:
                print(f"📋 STDOUT:\n{stdout.decode()}")
            if stderr:
                print(f"📋 STDERR:\n{stderr.decode()}")
        except subprocess.TimeoutExpired:
            print("⚠️ Process still running, cannot capture output")
        except Exception as e:
            print(f"⚠️ Error capturing output: {e}")
    
    def start(self, daemon: bool = True, restart: bool = True, devtools: bool = False) -> bool:
        """Start Continuum service with clean entry point"""
        continuum_path = self.find_continuum_executable()
        if not continuum_path:
            raise FileNotFoundError(
                "Continuum service not found! "
                "Please ensure 'continuum' CLI is accessible in the project root"
            )
        
        print(f"🚀 Starting Continuum service (like Docker Desktop) on port {self.port}...")
        if devtools:
            print("🔧 Enhanced mode: DevTools + AI monitoring")
        
        try:
            # Check if WebSocket server is already running
            if self.is_websocket_server_healthy(timeout=3):
                print(f"✅ Continuum service already running on port {self.port}")
                return True
            
            # Kill any existing servers
            if restart:
                print("🔄 Cleaning up existing services...")
                self.kill_existing_server()
            
            # Start the clean Continuum service
            cmd = [str(continuum_path), 'devtools' if devtools else 'start']
            
            print(f"🚀 Executing: {' '.join(cmd)}")
            print(f"📁 Working directory: {continuum_path.parent}")
            
            self.process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                cwd=continuum_path.parent,  # Run from continuum directory
                preexec_fn=os.setsid if hasattr(os, 'setsid') else None
            )
            
            # Monitor process output for startup confirmation
            print("⏳ Waiting for Continuum service to start...")
            startup_success = self.wait_for_typescript_startup(timeout=30)
            
            if startup_success and self.is_websocket_server_healthy(timeout=10):
                print("✅ Continuum service is ready!")
                return True
            else:
                # Debug: show process output
                print("❌ Continuum service failed to start properly")
                self.show_process_output()
                return False
                if self.process:
                    stdout, stderr = self.process.communicate(timeout=5)
                    print(f"❌ Server failed to start")
                    print(f"STDOUT: {stdout.decode()}")
                    print(f"STDERR: {stderr.decode()}")
                    print(f"Return code: {self.process.returncode}")
                self.stop()
                return False
                
        except subprocess.TimeoutExpired:
            print("⏱️ Server start timeout, checking health...")
            return self.is_server_healthy(timeout=10)
        except Exception as e:
            print(f"❌ Exception starting server: {e}")
            self.stop()
            return False
    
    def stop(self):
        """Stop the Continuum server"""
        if self.process:
            try:
                if hasattr(os, 'killpg'):
                    os.killpg(os.getpgid(self.process.pid), signal.SIGTERM)
                else:
                    self.process.terminate()
                
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                if hasattr(os, 'killpg'):
                    os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
                else:
                    self.process.kill()
            except Exception:
                pass
            finally:
                self.process = None
    
    def restart(self) -> bool:
        """Restart the server"""
        self.stop()
        return self.start()
    
    def __enter__(self):
        """Context manager entry"""
        if not self.start():
            raise RuntimeError("Failed to start Continuum server")
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit"""
        self.stop()