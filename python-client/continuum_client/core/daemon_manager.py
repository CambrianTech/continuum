"""
Daemon Manager - Object-Oriented Daemon Management System
Manages multiple daemon types with unified logging and inspection
"""

import asyncio
import json
from abc import ABC, abstractmethod
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Optional, Any
from collections import deque
import os
import signal


class BaseDaemon(ABC):
    """Abstract base class for all daemons"""
    
    def __init__(self, daemon_type: str, daemon_id: Optional[str] = None):
        self.daemon_type = daemon_type
        self.daemon_id = daemon_id or f"{daemon_type}-{datetime.now().strftime('%H%M%S')}"
        self.running = True
        self.memory_logs = deque(maxlen=100)
        self.start_time = datetime.now()
        
        # Setup logging (initialize log_file first, then write startup log)
        self.log_file = self._init_log_file()
        self.write_log("DAEMON_START", f"Started {self.daemon_type} daemon {self.daemon_id}")
        
    def _init_log_file(self) -> Path:
        """Initialize daemon-specific logging directory and file"""
        # Find .continuum directory
        base_dir = Path.cwd()
        while base_dir != base_dir.parent:
            if (base_dir / '.continuum').exists():
                break
            base_dir = base_dir.parent
        
        # Create daemon logs directory structure
        daemon_logs_dir = base_dir / '.continuum' / 'daemons' / self.daemon_type
        daemon_logs_dir.mkdir(parents=True, exist_ok=True)
        
        log_file = daemon_logs_dir / f"{self.daemon_id}.log"
        return log_file
    
    def setup_logging(self) -> Path:
        """Deprecated - use _init_log_file instead"""
        return self._init_log_file()
    
    def write_log(self, level: str, message: str, data: Optional[Dict] = None):
        """Write structured log entry"""
        try:
            timestamp = datetime.now().isoformat()
            log_entry = {
                'timestamp': timestamp,
                'daemon_id': self.daemon_id,
                'daemon_type': self.daemon_type,
                'level': level,
                'message': message,
                'data': data or {}
            }
            
            # Add to memory for fast access
            self.memory_logs.append(log_entry)
            
            # Write to file
            with open(self.log_file, 'a') as f:
                f.write(json.dumps(log_entry) + '\n')
                
        except Exception as e:
            print(f"🚨 Daemon logging error: {e}")
    
    def get_logs(self, lines: int = 50) -> List[Dict]:
        """Get recent daemon log entries"""
        try:
            if not self.log_file.exists():
                return []
                
            with open(self.log_file, 'r') as f:
                all_lines = f.readlines()
                recent_lines = all_lines[-lines:] if len(all_lines) >= lines else all_lines
                
                logs = []
                for line in recent_lines:
                    try:
                        logs.append(json.loads(line.strip()))
                    except json.JSONDecodeError:
                        continue
                        
                return logs
                
        except Exception as e:
            return [{'error': f"Error reading daemon logs: {e}"}]
    
    def get_status(self) -> Dict[str, Any]:
        """Get current daemon status"""
        uptime = datetime.now() - self.start_time
        return {
            'daemon_id': self.daemon_id,
            'daemon_type': self.daemon_type,
            'running': self.running,
            'uptime_seconds': uptime.total_seconds(),
            'log_file': str(self.log_file),
            'memory_logs_count': len(self.memory_logs),
            'start_time': self.start_time.isoformat()
        }
    
    def setup_signal_handlers(self):
        """Setup graceful shutdown signal handlers"""
        def signal_handler(signum, frame):
            self.write_log("SHUTDOWN", f"Received signal {signum}")
            self.running = False
            
        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)
    
    @abstractmethod
    async def run(self):
        """Main daemon execution loop - must be implemented by subclasses"""
        pass
    
    async def start(self):
        """Start the daemon with proper setup"""
        self.setup_signal_handlers()
        self.write_log("STARTING", "Daemon initialization complete")
        
        try:
            await self.run()
        except Exception as e:
            self.write_log("ERROR", f"Daemon crashed: {e}")
            raise
        finally:
            self.write_log("STOPPED", "Daemon shutdown complete")


class DaemonManager:
    """Manages multiple daemon instances with unified inspection"""
    
    def __init__(self):
        self.active_daemons: Dict[str, BaseDaemon] = {}
        self.daemon_processes: Dict[str, asyncio.Task] = {}
        
    def register_daemon(self, daemon: BaseDaemon) -> str:
        """Register a daemon instance"""
        daemon_id = daemon.daemon_id
        self.active_daemons[daemon_id] = daemon
        return daemon_id
    
    async def start_daemon(self, daemon: BaseDaemon) -> str:
        """Start a daemon in background task"""
        daemon_id = self.register_daemon(daemon)
        
        # Start daemon as background task
        task = asyncio.create_task(daemon.start())
        self.daemon_processes[daemon_id] = task
        
        print(f"🚀 Started daemon: {daemon.daemon_type} ({daemon_id})")
        return daemon_id
    
    def stop_daemon(self, daemon_id: str) -> bool:
        """Stop a specific daemon"""
        if daemon_id in self.active_daemons:
            daemon = self.active_daemons[daemon_id]
            daemon.running = False
            daemon.write_log("STOP_REQUESTED", "Daemon stop requested")
            
            # Cancel the task
            if daemon_id in self.daemon_processes:
                self.daemon_processes[daemon_id].cancel()
                del self.daemon_processes[daemon_id]
            
            del self.active_daemons[daemon_id]
            print(f"🛑 Stopped daemon: {daemon_id}")
            return True
            
        return False
    
    def get_daemon_status(self, daemon_id: str) -> Optional[Dict[str, Any]]:
        """Get status of specific daemon"""
        if daemon_id in self.active_daemons:
            return self.active_daemons[daemon_id].get_status()
        return None
    
    def get_daemon_logs(self, daemon_id: str, lines: int = 50) -> List[Dict]:
        """Get logs from specific daemon"""
        if daemon_id in self.active_daemons:
            return self.active_daemons[daemon_id].get_logs(lines)
        return []
    
    def list_daemons(self) -> List[Dict[str, Any]]:
        """List all active daemons"""
        return [daemon.get_status() for daemon in self.active_daemons.values()]
    
    def get_all_logs(self, lines: int = 20) -> Dict[str, List[Dict]]:
        """Get recent logs from all daemons"""
        all_logs = {}
        for daemon_id, daemon in self.active_daemons.items():
            all_logs[daemon_id] = daemon.get_logs(lines)
        return all_logs
    
    def find_daemons_by_type(self, daemon_type: str) -> List[str]:
        """Find daemon IDs by type"""
        return [
            daemon_id for daemon_id, daemon in self.active_daemons.items()
            if daemon.daemon_type == daemon_type
        ]


# Global daemon manager instance
daemon_manager = DaemonManager()