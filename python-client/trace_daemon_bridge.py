#!/usr/bin/env python3
"""
Trace Daemon Bridge - Find exactly where daemon connector breaks
"""

import asyncio
import json
import websockets
from datetime import datetime

class DaemonBridgeTracer:
    """Trace message flow through daemon system"""
    
    def __init__(self):
        self.ws = None
        self.client_id = None
        self.trace_log = []
    
    def log_trace(self, event: str, data: dict = None):
        """Log each step of the trace"""
        entry = {
            "timestamp": datetime.now().isoformat(),
            "event": event,
            "data": data
        }
        self.trace_log.append(entry)
        print(f"🔍 TRACE: {event}")
        if data:
            print(f"    📊 Data: {json.dumps(data, indent=2)}")
    
    async def connect_and_trace(self):
        """Connect and trace daemon bridge status"""
        self.log_trace("Starting WebSocket connection")
        
        self.ws = await websockets.connect("ws://localhost:9000")
        self.log_trace("WebSocket connected")
        
        # Client init
        init_message = {
            "type": "client_init",
            "data": {
                "userAgent": "Daemon Bridge Tracer",
                "url": "python://trace",
                "timestamp": datetime.now().isoformat()
            }
        }
        
        await self.ws.send(json.dumps(init_message))
        self.log_trace("Sent client_init", init_message)
        
        # Get connection confirmation
        response = await self.ws.recv()
        message = json.loads(response)
        self.log_trace("Received connection_confirmed", message)
        
        if message.get("type") == "connection_confirmed":
            self.client_id = message.get("data", {}).get("clientId")
            return True
        return False
    
    async def trace_daemon_status(self):
        """Get detailed daemon connector status"""
        self.log_trace("Requesting daemon status via get_stats")
        
        stats_message = {
            "type": "get_stats",
            "timestamp": datetime.now().isoformat(),
            "clientId": self.client_id
        }
        
        await self.ws.send(json.dumps(stats_message))
        response = await self.ws.recv()
        stats = json.loads(response)
        
        # Extract daemon connector details
        daemon_connector = stats.get("data", {}).get("daemonConnector", {})
        dynamic_router = stats.get("data", {}).get("dynamicRouter", {})
        
        self.log_trace("Daemon connector status", {
            "connected": daemon_connector.get("connected"),
            "commands_available": daemon_connector.get("commandsAvailable"),
            "registered_daemons": dynamic_router.get("registeredDaemons", []),
            "total_message_types": dynamic_router.get("totalMessageTypes")
        })
        
        return daemon_connector, dynamic_router
    
    async def trace_command_execution(self):
        """Trace what happens when we try to execute a command"""
        self.log_trace("Attempting command execution: health")
        
        command_message = {
            "type": "execute_command",
            "data": {
                "command": "health",
                "args": {}
            },
            "timestamp": datetime.now().isoformat(),
            "clientId": self.client_id
        }
        
        await self.ws.send(json.dumps(command_message))
        self.log_trace("Sent execute_command", command_message)
        
        # Wait for response with timeout
        try:
            response = await asyncio.wait_for(self.ws.recv(), timeout=5.0)
            result = json.loads(response)
            self.log_trace("Command execution response", result)
            return result
        except asyncio.TimeoutError:
            self.log_trace("Command execution TIMEOUT - no response in 5 seconds")
            return None
    
    async def trace_available_message_types(self):
        """Check what message types are actually available"""
        self.log_trace("Requesting available capabilities")
        
        capabilities_message = {
            "type": "get_capabilities",
            "timestamp": datetime.now().isoformat(),
            "clientId": self.client_id
        }
        
        await self.ws.send(json.dumps(capabilities_message))
        response = await self.ws.recv()
        capabilities = json.loads(response)
        
        self.log_trace("Available capabilities", capabilities)
        return capabilities
    
    async def full_trace(self):
        """Complete daemon bridge trace"""
        print("🔍 Starting Complete Daemon Bridge Trace")
        print("=" * 60)
        
        # Step 1: Connect
        if not await self.connect_and_trace():
            self.log_trace("FAILED: Could not establish connection")
            return
        
        # Step 2: Check daemon status
        daemon_connector, dynamic_router = await self.trace_daemon_status()
        
        # Step 3: Check available message types
        capabilities = await self.trace_available_message_types()
        
        # Step 4: Try command execution
        command_result = await self.trace_command_execution()
        
        # Step 5: Analysis
        await self.analyze_results(daemon_connector, dynamic_router, capabilities, command_result)
        
        await self.ws.close()
    
    async def analyze_results(self, daemon_connector, dynamic_router, capabilities, command_result):
        """Analyze trace results to identify the problem"""
        print("\n🔬 TRACE ANALYSIS")
        print("=" * 30)
        
        # Problem identification
        problems = []
        
        if not daemon_connector.get("connected", False):
            problems.append("❌ DAEMON CONNECTOR: Not connected to Command Processor")
        
        if daemon_connector.get("commandsAvailable", 0) == 0:
            problems.append("❌ NO COMMANDS: Zero commands available from Command Processor")
        
        if not command_result:
            problems.append("❌ COMMAND TIMEOUT: Commands not being processed")
        elif command_result.get("type") == "error":
            problems.append(f"❌ COMMAND ERROR: {command_result.get('data', {}).get('error')}")
        
        if len(dynamic_router.get("registeredDaemons", [])) <= 1:
            problems.append("❌ ISOLATED DAEMON: Only WebSocket daemon registered")
        
        if problems:
            print("🚨 PROBLEMS IDENTIFIED:")
            for problem in problems:
                print(f"  {problem}")
        else:
            print("✅ No obvious problems found")
        
        # Root cause hypothesis
        print("\n💡 ROOT CAUSE HYPOTHESIS:")
        if not daemon_connector.get("connected"):
            print("  🎯 WebSocket Daemon cannot reach Command Processor Daemon")
            print("  🔧 Likely causes:")
            print("     - Command Processor not running")
            print("     - IPC connection broken between daemons") 
            print("     - Daemon discovery/registration failed")
            print("     - Port/socket binding conflicts")
        
        # Next steps
        print("\n📋 NEXT DEBUGGING STEPS:")
        print("  1. Check if Command Processor daemon is actually running")
        print("  2. Check daemon startup logs for connection errors")
        print("  3. Verify IPC mechanism between WebSocket and Command Processor")
        print("  4. Test direct Command Processor communication")

async def main():
    """Run the complete daemon bridge trace"""
    tracer = DaemonBridgeTracer()
    await tracer.full_trace()
    
    # Save trace log for analysis
    with open("daemon_bridge_trace.json", "w") as f:
        json.dump(tracer.trace_log, f, indent=2)
    
    print(f"\n📝 Complete trace saved to: daemon_bridge_trace.json")

if __name__ == "__main__":
    asyncio.run(main())