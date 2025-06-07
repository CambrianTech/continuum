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
    
    def __init__(self, port: int = 5555):
        self.port = port
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
        """Start Continuum server"""
        continuum_path = self.find_continuum_executable()
        if not continuum_path:
            raise FileNotFoundError(
                "Continuum CLI not found! "
                "Please ensure continuum.cjs is accessible or install globally"
            )
        
        try:
            # Try restart first if requested
            if restart:
                restart_cmd = ['node', str(continuum_path), '--restart', '--port', str(self.port)]
                subprocess.run(restart_cmd, capture_output=True, text=True, timeout=10)
            
            # Start the server
            cmd = ['node', str(continuum_path), '--port', str(self.port)]
            if daemon:
                cmd.extend(['--daemon', '--idle-timeout', '0'])
            
            self.process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                preexec_fn=os.setsid if hasattr(os, 'setsid') else None
            )
            
            # Wait for server to be ready
            if self.is_server_healthy(timeout=30):
                return True
            else:
                self.stop()
                return False
                
        except subprocess.TimeoutExpired:
            return self.is_server_healthy(timeout=10)
        except Exception:
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