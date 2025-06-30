#!/usr/bin/env python3
"""
Dynamic Portal - AI Development Interface with Dynamic Command Discovery
True middle-out architecture with dynamic capabilities from the running system
"""

import asyncio
import json
import time
import requests
from datetime import datetime
from pathlib import Path
from typing import Dict, Any, Optional, List
from dataclasses import dataclass
import logging

# ============================================================================
# DYNAMIC DISCOVERY LAYER - Learn from Running System
# ============================================================================

class DynamicCapabilities:
    """Discovers capabilities from the running Continuum system"""
    
    def __init__(self, base_url: str):
        self.base_url = base_url
        self.capabilities: Dict[str, Any] = {}
        self.commands: Dict[str, Dict] = {}
        
    async def discover(self):
        """Discover what the running system can do"""
        # Discover available commands
        await self._discover_commands()
        # Discover available APIs
        await self._discover_apis()
        # Discover daemon capabilities
        await self._discover_daemons()
        
    async def _discover_commands(self):
        """Find all available commands dynamically"""
        try:
            response = requests.get(f"{self.base_url}/api/commands/list", timeout=10)
            if response.status_code == 200:
                self.commands = response.json()
                print(f"📋 Discovered {len(self.commands)} commands")
        except:
            # Fallback - discover through health endpoint
            try:
                response = requests.get(f"{self.base_url}/health", timeout=10)
                if response.status_code == 200:
                    # At minimum we know health works
                    self.commands["health"] = {"description": "System health check"}
            except:
                pass
                
    async def _discover_apis(self):
        """Find all available API endpoints"""
        try:
            response = requests.get(f"{self.base_url}/api/endpoints", timeout=10)
            if response.status_code == 200:
                self.capabilities["apis"] = response.json()
        except:
            # Fallback - probe common endpoints
            endpoints = ["/health", "/api/browser/screenshot", "/api/console/logs", "/api/daemons"]
            working_endpoints = []
            for endpoint in endpoints:
                try:
                    response = requests.get(f"{self.base_url}{endpoint}", timeout=5)
                    if response.status_code in [200, 404]:  # 404 means endpoint exists but may need params
                        working_endpoints.append(endpoint)
                except:
                    pass
            self.capabilities["discovered_endpoints"] = working_endpoints
            
    async def _discover_daemons(self):
        """Find running daemons and their capabilities"""
        try:
            response = requests.get(f"{self.base_url}/api/daemons", timeout=10)
            if response.status_code == 200:
                daemon_info = response.json()
                self.capabilities["daemons"] = daemon_info
                print(f"🔧 Discovered {len(daemon_info.get('router', {}).get('daemons', []))} daemons")
        except:
            pass

# ============================================================================
# DYNAMIC COMMAND INTERFACE - Execute Any Discovered Command
# ============================================================================

class DynamicCommands:
    """Execute commands discovered from the system"""
    
    def __init__(self, base_url: str, capabilities: DynamicCapabilities):
        self.base_url = base_url
        self.capabilities = capabilities
        
    async def execute(self, command_name: str, **kwargs) -> Any:
        """Execute any discovered command"""
        if command_name not in self.capabilities.commands:
            # Try anyway - system might have new commands
            pass
            
        try:
            response = requests.post(f"{self.base_url}/api/command", 
                                   json={"command": command_name, "args": kwargs}, 
                                   timeout=30)
            if response.status_code == 200:
                return response.json()
            else:
                return {"error": f"Command failed with status {response.status_code}"}
        except Exception as e:
            return {"error": str(e)}
    
    def __getattr__(self, name: str):
        """Dynamic method creation - continuum.health() becomes continuum.execute('health')"""
        async def dynamic_method(**kwargs):
            return await self.execute(name, **kwargs)
        return dynamic_method

# ============================================================================
# SESSION MANAGEMENT - Development History
# ============================================================================

class SessionManager:
    """Manages development sessions with artifact storage"""
    
    def __init__(self):
        self.session_id = datetime.now().strftime("%Y%m%d_%H%M%S")
        self.session_dir = Path(f"portal_sessions/{self.session_id}")
        self.session_dir.mkdir(parents=True, exist_ok=True)
        self.actions: List[Dict] = []
    
    def log_action(self, action: str, data: Any = None):
        """Log an action to the session"""
        entry = {
            "timestamp": datetime.now().isoformat(),
            "action": action,
            "data": data
        }
        self.actions.append(entry)
        
        # Save to file
        with open(self.session_dir / "actions.jsonl", "a") as f:
            f.write(json.dumps(entry) + "\n")
    
    def save_artifact(self, name: str, content: Any, artifact_type: str = "text"):
        """Save an artifact (screenshot, code, logs, etc.)"""
        artifact_path = self.session_dir / name
        
        if artifact_type == "image" and isinstance(content, bytes):
            artifact_path.write_bytes(content)
        else:
            artifact_path.write_text(str(content))
        
        self.log_action("artifact_saved", {
            "name": name,
            "type": artifact_type,
            "size": len(content) if isinstance(content, (str, bytes)) else 0
        })
        return artifact_path

# ============================================================================
# DYNAMIC PORTAL - AI-Native Interface
# ============================================================================

class DynamicPortal:
    """
    AI-native development portal with dynamic capability discovery
    Adapts to whatever the running Continuum system provides
    """
    
    def __init__(self, base_url: str = "http://localhost:9000"):
        self.base_url = base_url
        self.session = SessionManager()
        self.capabilities = DynamicCapabilities(base_url)
        self.commands = None
        
        print(f"🚀 Dynamic Portal initialized - Session: {self.session.session_id}")
    
    async def connect(self):
        """Connect and discover system capabilities"""
        try:
            # Test basic connectivity
            response = requests.get(f"{self.base_url}/health", timeout=10)
            if response.status_code != 200:
                raise ConnectionError("Health check failed")
            
            # Discover what the system can do
            await self.capabilities.discover()
            
            # Create dynamic command interface
            self.commands = DynamicCommands(self.base_url, self.capabilities)
            
            self.session.log_action("portal_connected", {
                "endpoint": self.base_url,
                "commands_discovered": len(self.capabilities.commands),
                "capabilities": list(self.capabilities.capabilities.keys())
            })
            
            print(f"📡 Connected to Continuum at {self.base_url}")
            print(f"🔍 Discovered {len(self.capabilities.commands)} commands")
            return self
            
        except Exception as e:
            raise ConnectionError(f"Failed to connect: {e}")
    
    # ========== DYNAMIC API ACCESS ==========
    
    async def api_call(self, endpoint: str, method: str = "GET", **kwargs) -> Any:
        """Make dynamic API calls to discovered endpoints"""
        try:
            url = f"{self.base_url}{endpoint}"
            if method.upper() == "GET":
                response = requests.get(url, params=kwargs, timeout=10)
            elif method.upper() == "POST":
                response = requests.post(url, json=kwargs, timeout=10)
            else:
                raise ValueError(f"Unsupported method: {method}")
            
            if response.status_code == 200:
                try:
                    return response.json()
                except:
                    return response.text
            else:
                return {"error": f"API call failed with status {response.status_code}"}
        except Exception as e:
            return {"error": str(e)}
    
    async def screenshot(self, description: str = "debug") -> Optional[bytes]:
        """Take screenshot using discovered API"""
        try:
            response = requests.get(f"{self.base_url}/api/browser/screenshot", timeout=10)
            if response.status_code == 200:
                screenshot_data = response.content
                
                # Save as artifact
                filename = f"screenshot_{description}_{int(time.time())}.png"
                self.session.save_artifact(filename, screenshot_data, "image")
                
                self.session.log_action("screenshot_taken", {
                    "description": description,
                    "filename": filename,
                    "size": len(screenshot_data)
                })
                
                print(f"📸 Screenshot saved: {filename}")
                return screenshot_data
            return None
        except Exception as e:
            print(f"❌ Screenshot failed: {e}")
            return None
    
    # ========== AI-NATIVE WORKFLOWS ==========
    
    async def debug_workflow(self) -> Dict[str, Any]:
        """Complete debugging workflow using discovered capabilities"""
        print("🔍 Running dynamic debug workflow...")
        
        results = {}
        
        # Use discovered capabilities dynamically
        if "health" in self.capabilities.commands:
            results["health"] = await self.commands.health()
        
        # Try screenshot if available
        screenshot = await self.screenshot("debug_workflow")
        results["screenshot_captured"] = screenshot is not None
        
        # Try to get daemon status
        daemon_status = await self.api_call("/api/daemons")
        if "error" not in daemon_status:
            results["daemons"] = daemon_status
        
        # Try to get console logs
        console_logs = await self.api_call("/api/console/logs", limit=10)
        if "error" not in console_logs:
            results["console_logs"] = console_logs
        
        workflow_result = {
            "timestamp": datetime.now().isoformat(),
            "capabilities_used": list(results.keys()),
            "success_count": len([k for k, v in results.items() if "error" not in str(v)])
        }
        
        self.session.log_action("debug_workflow_completed", workflow_result)
        print(f"✅ Debug workflow complete: {workflow_result}")
        return {"workflow": workflow_result, "results": results}
    
    async def close(self):
        """Clean shutdown and session summary"""
        summary = {
            "session_id": self.session.session_id,
            "total_actions": len(self.session.actions),
            "capabilities_discovered": len(self.capabilities.commands),
            "end_time": datetime.now().isoformat()
        }
        
        summary_file = self.session.session_dir / "session_summary.json"
        summary_file.write_text(json.dumps(summary, indent=2))
        
        print(f"📋 Session complete: {self.session.session_dir}")
        print(f"📊 Summary: {summary['total_actions']} actions, {summary['capabilities_discovered']} capabilities")

# ============================================================================
# MAIN DEMO - Prove Dynamic Discovery Works
# ============================================================================

async def main():
    """Demonstrate dynamic portal capabilities"""
    print("🌟 Dynamic Portal - AI Development with Dynamic Discovery")
    print("=" * 60)
    
    # Initialize and connect
    portal = DynamicPortal()
    await portal.connect()
    
    print("\n🎯 Dynamic Capabilities:")
    print(f"  📋 Commands: {list(portal.capabilities.commands.keys())}")
    print(f"  🔌 APIs: {list(portal.capabilities.capabilities.get('discovered_endpoints', []))}")
    
    print("\n🔍 Testing Dynamic Command Execution:")
    
    # Test dynamic command execution
    if hasattr(portal.commands, 'health'):
        health_result = await portal.commands.health()
        print(f"  ✅ Dynamic health(): {health_result}")
    
    # Test debug workflow
    print("\n🤖 AI Debug Workflow:")
    debug_result = await portal.debug_workflow()
    print(f"  🔍 Workflow used {debug_result['workflow']['capabilities_used']}")
    
    # Clean shutdown
    await portal.close()
    
    print("\n✨ Dynamic Portal demo complete!")
    print("🎯 System adapts to whatever Continuum provides!")

if __name__ == "__main__":
    asyncio.run(main())