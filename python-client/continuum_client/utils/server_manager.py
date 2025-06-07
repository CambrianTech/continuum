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
        """Find the Continuum CLI executable"""
        # Start from python-client and go up to find continuum.cjs
        current_dir = Path(__file__).parent
        
        # Go up from continuum_client/utils/ to find continuum.cjs
        while current_dir.name != '/' and len(current_dir.parts) > 1:
            continuum_path = current_dir / 'continuum.cjs'
            if continuum_path.exists():
                return continuum_path
            current_dir = current_dir.parent
        
        # Try global node command
        try:
            result = subprocess.run(['which', 'continuum'], capture_output=True, text=True)
            if result.returncode == 0:
                return Path(result.stdout.strip())
        except:
            pass
        
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
    
    def start(self, daemon: bool = True, restart: bool = True) -> bool:
        """Start Continuum server with robust error handling"""
        continuum_path = self.find_continuum_executable()
        if not continuum_path:
            raise FileNotFoundError(
                "Continuum CLI not found! "
                "Please ensure continuum.cjs is accessible or install globally"
            )
        
        print(f"🔧 Starting Continuum server on port {self.port}...")
        
        try:
            # Check if server is already running
            if self.is_server_healthy(timeout=3):
                print(f"✅ Server already running on port {self.port}")
                return True
            
            # Kill any existing servers and start fresh
            if restart:
                print("🔄 Cleaning up existing servers...")
                self.kill_existing_server()
            
            # Start the server
            cmd = ['node', str(continuum_path), '--port', str(self.port)]
            if daemon:
                cmd.extend(['--daemon', '--idle-timeout', '0'])
            
            print(f"🚀 Executing: {' '.join(cmd)}")
            
            self.process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                cwd=continuum_path.parent,  # Run from continuum directory
                preexec_fn=os.setsid if hasattr(os, 'setsid') else None
            )
            
            # Give daemon mode time to start
            if daemon:
                import time
                time.sleep(3)
            
            # Wait for server to be ready
            print("⏳ Waiting for server to be ready...")
            if self.is_server_healthy(timeout=30):
                print("✅ Continuum server is ready!")
                return True
            else:
                # Debug: show process output
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