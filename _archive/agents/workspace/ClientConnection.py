#!/usr/bin/env python3
"""
ClientConnection - Core Connection & Validation Framework
========================================================

Base class for all client connections to Continuum.
This will be converted to TypeScript later.

Client Types & Their Consoles:
- BrowserClientConnection: WebSocket → Browser JavaScript console
- TerminalClientConnection: Stdio/WebSocket → Terminal/shell console  
- AgentClientConnection: WebSocket → AI agent conversation console
- APIClientConnection: HTTP/REST → API response/error console
- DeviceClientConnection: Platform-specific → Mobile/desktop app console

Usage:
    browser = BrowserClientConnection()
    agent = AgentClientConnection()
    terminal = TerminalClientConnection()
    
    await browser.connect()
    await agent.connect()
    await terminal.connect()
    
    # Each has different console capabilities
    browser_result = await browser.execute_js("console.log('test')")
    agent_result = await agent.send_message("debug the browser connection")
    terminal_result = await terminal.execute_command("ls -la")
"""

import asyncio
import websockets
import json
import base64
import os
from datetime import datetime
from typing import Dict, List, Optional, Any

class ClientConnection:
    """Base class for all Continuum client connections"""
    def __init__(self, connection_url: str = "ws://localhost:9000"):
        self.connection_url = connection_url
        self.connection = None
        self.connected = False
        self.version_expected = None
        self.version_client = None
        self.client_type = "base"
        
    async def connect(self) -> bool:
        """Connect to Continuum - implemented by subclasses"""
        raise NotImplementedError("Subclass must implement connect()")
    
    async def disconnect(self):
        """Disconnect from Continuum - implemented by subclasses"""
        raise NotImplementedError("Subclass must implement disconnect()")
    
    async def execute_command(self, command: str, **kwargs) -> Dict[str, Any]:
        """Execute command - implemented by subclasses"""
        raise NotImplementedError("Subclass must implement execute_command()")
    
    async def validate_client_specific(self) -> Dict[str, Any]:
        """Run client-specific validation - implemented by subclasses"""
        raise NotImplementedError("Subclass must implement validate_client_specific()")

class BrowserClientConnection(ClientConnection):
    """WebSocket connection to browser tabs with JavaScript console"""
    def __init__(self, ws_url: str = "ws://localhost:9000"):
        super().__init__(ws_url)
        self.websocket = None
        self.client_type = "browser"
        
    async def connect(self) -> bool:
        """Establish WebSocket connection"""
        try:
            self.websocket = await websockets.connect(self.connection_url)
            self.connection = self.websocket
            self.connected = True
            
            # Skip connection banner
            try:
                await asyncio.wait_for(self.websocket.recv(), timeout=3.0)
            except:
                pass
                
            return True
        except Exception as e:
            print(f"❌ Browser connection failed: {e}")
            return False
    
    async def disconnect(self):
        """Close WebSocket connection"""
        if self.websocket:
            await self.websocket.close()
            self.connected = False
            self.connection = None
    
    async def execute_js(self, js_code: str, timeout: float = 10.0) -> Dict[str, Any]:
        """Execute JavaScript and return results with console output"""
        if not self.connected:
            return {"success": False, "error": "Not connected"}
        
        try:
            encoded_js = base64.b64encode(js_code.encode()).decode()
            task_message = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded_js}'
            }
            
            await self.websocket.send(json.dumps(task_message))
            
            # Wait for js_executed response
            for attempt in range(5):
                try:
                    response = await asyncio.wait_for(self.websocket.recv(), timeout=timeout/5)
                    result = json.loads(response)
                    
                    if result.get('type') == 'js_executed':
                        return {
                            "success": result.get('success', False),
                            "result": result.get('result'),
                            "output": result.get('output', []),
                            "error": result.get('error'),
                            "timestamp": result.get('timestamp')
                        }
                        
                except asyncio.TimeoutError:
                    continue
            
            return {"success": False, "error": "Timeout waiting for execution"}
            
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    async def capture_console_output(self, js_code: str) -> Dict[str, Any]:
        """Execute JS and capture console output with categorization"""
        result = await self.execute_js(js_code)
        
        if not result["success"]:
            return result
        
        # Categorize console output
        output = result.get("output", [])
        errors = [entry for entry in output if entry.get("level") == "error"]
        warnings = [entry for entry in output if entry.get("level") == "warn"]
        logs = [entry for entry in output if entry.get("level") == "log"]
        
        return {
            "success": True,
            "result": result.get("result"),
            "console": {
                "total": len(output),
                "errors": errors,
                "warnings": warnings,
                "logs": logs,
                "raw": output
            }
        }
    
    async def validate_error_systems(self) -> Dict[str, Any]:
        """MILESTONE 1: Validate error detection and console capture"""
        test_js = '''
        console.log("🧪 Testing error systems...");
        console.error("TEST_ERROR: Error detection test");
        console.warn("TEST_WARNING: Warning detection test");
        console.log("✅ Error systems test complete");
        "ERROR_SYSTEMS_VALIDATED";
        '''
        
        result = await self.capture_console_output(test_js)
        
        if result["success"]:
            console = result["console"]
            errors_found = len(console["errors"]) > 0
            warnings_found = len(console["warnings"]) > 0
            
            return {
                "milestone": 1,
                "success": errors_found and warnings_found,
                "errors_detected": len(console["errors"]),
                "warnings_detected": len(console["warnings"]),
                "console_output": console
            }
        
        return {"milestone": 1, "success": False, "error": result.get("error")}
    
    async def validate_console_reading(self) -> Dict[str, Any]:
        """MILESTONE 3: Validate console reading capability"""
        test_js = '''
        console.log("📖 Testing console reading...");
        console.error("CRITICAL_ERROR: Database connection failed");
        console.warn("PERFORMANCE_WARNING: Slow query detected");
        console.log("INFO: User authentication successful");
        console.log("✅ Console reading test complete");
        "CONSOLE_READING_VALIDATED";
        '''
        
        result = await self.capture_console_output(test_js)
        
        if result["success"]:
            console = result["console"]
            return {
                "milestone": 3,
                "success": console["total"] >= 4,
                "total_messages": console["total"],
                "categorized": {
                    "errors": len(console["errors"]),
                    "warnings": len(console["warnings"]),
                    "logs": len(console["logs"])
                },
                "console_output": console
            }
        
        return {"milestone": 3, "success": False, "error": result.get("error")}
    
    async def validate_error_feedback(self) -> Dict[str, Any]:
        """MILESTONE 4: Validate error feedback processing"""
        # First generate errors
        error_result = await self.validate_error_systems()
        
        if not error_result["success"]:
            return {"milestone": 4, "success": False, "error": "Could not generate errors"}
        
        # Process the feedback
        errors = error_result["console_output"]["errors"]
        warnings = error_result["console_output"]["warnings"]
        
        processed_feedback = {
            "error_types": [self._classify_error(e["message"]) for e in errors],
            "warning_types": [self._classify_warning(w["message"]) for w in warnings],
            "total_issues": len(errors) + len(warnings)
        }
        
        return {
            "milestone": 4,
            "success": len(processed_feedback["error_types"]) > 0,
            "feedback_processed": processed_feedback
        }
    
    async def validate_version_from_client(self) -> Dict[str, Any]:
        """MILESTONE 5: Get version feedback FROM client console"""
        version_js = '''
        console.log("🔍 Reading client version...");
        const version = window.CLIENT_VERSION || 
                       document.querySelector('[data-version]')?.dataset.version || 
                       "0.2.1973";
        console.log("📦 Client version:", version);
        JSON.stringify({
            clientVersion: version,
            timestamp: new Date().toISOString()
        });
        '''
        
        result = await self.execute_js(version_js)
        
        if result["success"]:
            try:
                version_data = json.loads(result["result"])
                self.version_client = version_data["clientVersion"]
                
                # Get expected version
                try:
                    with open("package.json", "r") as f:
                        package_data = json.load(f)
                        self.version_expected = package_data["version"]
                except:
                    self.version_expected = "unknown"
                
                return {
                    "milestone": 5,
                    "success": True,
                    "version_client": self.version_client,
                    "version_expected": self.version_expected,
                    "versions_match": self.version_client == self.version_expected
                }
            except:
                return {"milestone": 5, "success": False, "error": "Could not parse version data"}
        
        return {"milestone": 5, "success": False, "error": result.get("error")}
    
    async def capture_screenshot(self) -> Dict[str, Any]:
        """MILESTONE 6: Capture browser screenshot"""
        if not self.connected:
            return {"success": False, "error": "Not connected"}
        
        try:
            task_message = {
                'type': 'task',
                'role': 'system',
                'task': '[CMD:SCREENSHOT] {"format": "png", "fullPage": true}'
            }
            
            await self.websocket.send(json.dumps(task_message))
            
            # Wait for screenshot response
            for attempt in range(3):
                try:
                    response = await asyncio.wait_for(self.websocket.recv(), timeout=15.0)
                    result = json.loads(response)
                    
                    if 'screenshot saved' in str(result):
                        screenshot_path = result.get('message', '').split('screenshot saved: ')[-1]
                        
                        if os.path.exists(screenshot_path):
                            return {
                                "success": True,
                                "screenshot_path": screenshot_path,
                                "file_size": os.path.getsize(screenshot_path)
                            }
                    
                except asyncio.TimeoutError:
                    continue
            
            return {"success": False, "error": "Screenshot capture timeout"}
            
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    async def run_full_modem_protocol(self) -> Dict[str, Any]:
        """Run complete modem protocol validation"""
        if not await self.connect():
            return {"success": False, "error": "Could not connect"}
        
        results = {
            "timestamp": datetime.now().isoformat(),
            "milestones": {},
            "overall_success": False
        }
        
        try:
            # MILESTONE 1: Error Systems
            results["milestones"]["M1"] = await self.validate_error_systems()
            
            # MILESTONE 3: Console Reading (depends on M1)
            if results["milestones"]["M1"]["success"]:
                results["milestones"]["M3"] = await self.validate_console_reading()
            
            # MILESTONE 4: Error Feedback
            results["milestones"]["M4"] = await self.validate_error_feedback()
            
            # MILESTONE 5: Version from Client
            results["milestones"]["M5"] = await self.validate_version_from_client()
            
            # MILESTONE 6: Screenshot
            results["milestones"]["M6"] = await self.capture_screenshot()
            
            # Overall success calculation
            successful_milestones = sum(1 for m in results["milestones"].values() if m.get("success"))
            results["overall_success"] = successful_milestones >= 4  # Allow some failures
            results["success_rate"] = f"{successful_milestones}/{len(results['milestones'])}"
            
        finally:
            await self.disconnect()
        
        return results
    
    def _classify_error(self, error_msg: str) -> str:
        """Classify error type from message"""
        if "CRITICAL" in error_msg:
            return "critical"
        elif "SYNTAX" in error_msg:
            return "syntax"
        elif "NETWORK" in error_msg:
            return "network"
        else:
            return "general"
    
    def _classify_warning(self, warning_msg: str) -> str:
        """Classify warning type from message"""
        if "PERFORMANCE" in warning_msg:
            return "performance"
        elif "DEPRECATION" in warning_msg:
            return "deprecation"
        elif "MEMORY" in warning_msg:
            return "memory"
        else:
            return "general"

class AgentClientConnection(ClientConnection):
    """WebSocket connection to AI agents with conversation console"""
    def __init__(self, ws_url: str = "ws://localhost:9000", agent_name: str = "GeneralAI"):
        super().__init__(ws_url)
        self.websocket = None
        self.client_type = "agent"
        self.agent_name = agent_name
        self.conversation_history = []
        
    async def connect(self) -> bool:
        """Establish WebSocket connection to agent"""
        try:
            self.websocket = await websockets.connect(self.connection_url)
            self.connection = self.websocket
            self.connected = True
            
            # Skip connection banner
            try:
                await asyncio.wait_for(self.websocket.recv(), timeout=3.0)
            except:
                pass
                
            return True
        except Exception as e:
            print(f"❌ Agent connection failed: {e}")
            return False
    
    async def disconnect(self):
        """Close agent connection"""
        if self.websocket:
            await self.websocket.close()
            self.connected = False
            self.connection = None
    
    async def send_message(self, message: str, room: str = "general") -> Dict[str, Any]:
        """Send message to agent and capture response"""
        if not self.connected:
            return {"success": False, "error": "Not connected"}
        
        try:
            message_data = {
                'type': 'direct_message',
                'agent': self.agent_name,
                'content': message,
                'room': room
            }
            
            await self.websocket.send(json.dumps(message_data))
            
            # Wait for agent response
            try:
                response = await asyncio.wait_for(self.websocket.recv(), timeout=30.0)
                result = json.loads(response)
                
                # Log to conversation history
                self.conversation_history.append({
                    "timestamp": datetime.now().isoformat(),
                    "message": message,
                    "response": result.get("message", ""),
                    "agent": result.get("agent", self.agent_name)
                })
                
                return {
                    "success": True,
                    "response": result.get("message", ""),
                    "agent": result.get("agent", self.agent_name),
                    "conversation_history": self.conversation_history
                }
                
            except asyncio.TimeoutError:
                return {"success": False, "error": "Agent response timeout"}
                
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    async def validate_client_specific(self) -> Dict[str, Any]:
        """Agent-specific validation - conversation capability"""
        test_result = await self.send_message("Hello, please confirm you're operational")
        
        return {
            "client_type": "agent",
            "agent_name": self.agent_name,
            "success": test_result["success"],
            "response_time": "< 30s" if test_result["success"] else "timeout", 
            "validation": "Agent communication validated" if test_result["success"] else "Agent not responding",
            "conversation_history": self.conversation_history
        }

class TerminalClientConnection(ClientConnection):
    """Connection to terminal/shell console via Continuum"""
    def __init__(self, connection_url: str = "ws://localhost:9000"):
        super().__init__(connection_url)
        self.websocket = None
        self.client_type = "terminal"
        self.command_history = []
        
    async def connect(self) -> bool:
        """Establish connection for terminal commands"""
        try:
            self.websocket = await websockets.connect(self.connection_url)
            self.connection = self.websocket
            self.connected = True
            
            # Skip connection banner
            try:
                await asyncio.wait_for(self.websocket.recv(), timeout=3.0)
            except:
                pass
                
            return True
        except Exception as e:
            print(f"❌ Terminal connection failed: {e}")
            return False
    
    async def disconnect(self):
        """Close terminal connection"""
        if self.websocket:
            await self.websocket.close()
            self.connected = False
            self.connection = None
    
    async def execute_command(self, command: str, timeout: float = 30.0) -> Dict[str, Any]:
        """Execute terminal/shell command"""
        if not self.connected:
            return {"success": False, "error": "Not connected"}
        
        try:
            task_message = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:EXEC] {command}'
            }
            
            await self.websocket.send(json.dumps(task_message))
            
            # Wait for command result
            try:
                response = await asyncio.wait_for(self.websocket.recv(), timeout=timeout)
                result = json.loads(response)
                
                # Log to command history
                self.command_history.append({
                    "timestamp": datetime.now().isoformat(),
                    "command": command,
                    "result": result
                })
                
                return {
                    "success": True,
                    "command": command,
                    "output": result.get("message", ""),
                    "command_history": self.command_history
                }
                
            except asyncio.TimeoutError:
                return {"success": False, "error": "Command execution timeout"}
                
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    async def validate_client_specific(self) -> Dict[str, Any]:
        """Terminal-specific validation - command execution capability"""
        test_result = await self.execute_command("echo 'Terminal validation test'")
        
        return {
            "client_type": "terminal", 
            "success": test_result["success"],
            "command_execution": "Working" if test_result["success"] else "Failed",
            "validation": "Terminal commands validated" if test_result["success"] else "Terminal commands not working",
            "command_history": self.command_history
        }

# Each client type has its own validation routines
async def validate_browser_client() -> Dict[str, Any]:
    """Run browser client validation (modem protocol)"""
    client = BrowserClientConnection()
    return await client.run_full_modem_protocol()

async def validate_agent_client(agent_name: str = "GeneralAI") -> Dict[str, Any]:
    """Run agent client validation"""
    client = AgentClientConnection(agent_name=agent_name)
    if not await client.connect():
        return {"success": False, "error": "Could not connect to agent"}
    
    try:
        return await client.validate_client_specific()
    finally:
        await client.disconnect()

async def validate_terminal_client() -> Dict[str, Any]:
    """Run terminal client validation"""
    client = TerminalClientConnection()
    if not await client.connect():
        return {"success": False, "error": "Could not connect for terminal commands"}
    
    try:
        return await client.validate_client_specific()
    finally:
        await client.disconnect()

# Convenience functions for easy usage
async def quick_js_execute(js_code: str) -> Dict[str, Any]:
    """Quick JavaScript execution"""
    client = BrowserClientConnection()
    if await client.connect():
        result = await client.execute_js(js_code)
        await client.disconnect()
        return result
    return {"success": False, "error": "Connection failed"}

async def quick_agent_message(message: str, agent_name: str = "GeneralAI") -> Dict[str, Any]:
    """Quick agent message"""
    client = AgentClientConnection(agent_name=agent_name)
    if await client.connect():
        result = await client.send_message(message)
        await client.disconnect()
        return result
    return {"success": False, "error": "Connection failed"}

async def quick_terminal_command(command: str) -> Dict[str, Any]:
    """Quick terminal command"""
    client = TerminalClientConnection()
    if await client.connect():
        result = await client.execute_command(command)
        await client.disconnect()
        return result
    return {"success": False, "error": "Connection failed"}

async def validate_all_clients() -> Dict[str, Any]:
    """Run validation for all client types - takes advantage of shared validations"""
    results = {
        "timestamp": datetime.now().isoformat(),
        "clients": {},
        "overall_success": False
    }
    
    # Browser client (full modem protocol)
    print("🌐 Validating Browser Client...")
    results["clients"]["browser"] = await validate_browser_client()
    
    # Agent client  
    print("🤖 Validating Agent Client...")
    results["clients"]["agent"] = await validate_agent_client()
    
    # Terminal client
    print("💻 Validating Terminal Client...")
    results["clients"]["terminal"] = await validate_terminal_client()
    
    # Overall success if majority pass
    successful_clients = sum(1 for c in results["clients"].values() if c.get("success"))
    total_clients = len(results["clients"])
    results["overall_success"] = successful_clients >= (total_clients // 2 + 1)
    results["success_rate"] = f"{successful_clients}/{total_clients}"
    
    return results

# CLI usage
if __name__ == "__main__":
    import sys
    
    async def main():
        if len(sys.argv) > 1:
            client_type = sys.argv[1].lower()
            
            if client_type == "browser":
                result = await validate_browser_client()
                print(f"Browser Client: {'SUCCESS' if result['overall_success'] else 'FAILED'}")
                if not result['overall_success']:
                    print(json.dumps(result, indent=2))
                    
            elif client_type == "agent":
                agent_name = sys.argv[2] if len(sys.argv) > 2 else "GeneralAI"
                result = await validate_agent_client(agent_name)
                print(f"Agent Client ({agent_name}): {'SUCCESS' if result['success'] else 'FAILED'}")
                if not result['success']:
                    print(json.dumps(result, indent=2))
                    
            elif client_type == "terminal":
                result = await validate_terminal_client()
                print(f"Terminal Client: {'SUCCESS' if result['success'] else 'FAILED'}")
                if not result['success']:
                    print(json.dumps(result, indent=2))
                    
            elif client_type == "all":
                results = await validate_all_clients()
                print(f"All Clients: {'SUCCESS' if results['overall_success'] else 'FAILED'}")
                print(f"Success Rate: {results['success_rate']}")
                for client_name, client_result in results["clients"].items():
                    status = "✅" if client_result.get("success") else "❌"
                    print(f"  {status} {client_name.title()}")
                    
            elif client_type == "js":
                if len(sys.argv) > 2:
                    js_code = " ".join(sys.argv[2:])
                    result = await quick_js_execute(js_code)
                    print(json.dumps(result, indent=2))
                    
            elif client_type == "agent-msg":
                if len(sys.argv) > 2:
                    message = " ".join(sys.argv[2:])
                    result = await quick_agent_message(message)
                    print(json.dumps(result, indent=2))
                    
            elif client_type == "cmd":
                if len(sys.argv) > 2:
                    command = " ".join(sys.argv[2:])
                    result = await quick_terminal_command(command)
                    print(json.dumps(result, indent=2))
                    
            else:
                print("Usage:")
                print("  python ClientConnection.py [browser|agent|terminal|all]")
                print("  python ClientConnection.py js <javascript_code>")
                print("  python ClientConnection.py agent-msg <message>")
                print("  python ClientConnection.py cmd <shell_command>")
                print("  python ClientConnection.py agent <agent_name>")
        else:
            # Default: validate all clients
            results = await validate_all_clients()
            print(f"All Client Validation: {'SUCCESS' if results['overall_success'] else 'FAILED'}")
            print(f"Success Rate: {results['success_rate']}")
    
    asyncio.run(main())