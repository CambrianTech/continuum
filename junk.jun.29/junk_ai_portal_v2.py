#!/usr/bin/env python3
"""
AI Portal v2 - Elegant Async Integration
Clean portal using the new async Continuum client
"""

import asyncio
import json
from datetime import datetime
from continuum_client.async_client import connect, EventType, LogLevel

class AIPortal:
    """
    AI development portal with elegant async integration
    Real-time JTAG hooks for autonomous development
    """
    
    def __init__(self):
        self.continuum = None
        self.session_log = []
        
    async def start(self):
        """Start the AI portal"""
        print("🤖 AI Portal v2 - Starting...")
        
        # Single elegant connect
        self.continuum = await connect()
        
        print("✅ Connected to Continuum system")
        
        # Start real-time monitoring
        await self._start_monitoring()
        
    async def _start_monitoring(self):
        """Start real-time system monitoring"""
        print("📡 Starting real-time monitoring...")
        
        # Create monitoring tasks
        tasks = [
            asyncio.create_task(self._monitor_console()),
            asyncio.create_task(self._monitor_daemons()),
            asyncio.create_task(self._health_check_loop()),
            asyncio.create_task(self._command_interface())
        ]
        
        # Run all monitoring concurrently
        await asyncio.gather(*tasks, return_exceptions=True)
        
    async def _monitor_console(self):
        """Monitor console logs in real-time"""
        try:
            async for log in self.continuum.console_stream():
                timestamp = datetime.now().strftime("%H:%M:%S")
                
                # Log important events
                if log.level in ["error", "warn"]:
                    self.session_log.append({
                        "time": timestamp,
                        "type": "console",
                        "level": log.level,
                        "source": log.source,
                        "message": log.data.get('message', '')
                    })
                    
                    print(f"🚨 [{timestamp}] Console {log.level}: {log.source} - {log.data.get('message', '')[:80]}")
                    
                    # Auto-response for errors
                    if log.level == "error":
                        await self._handle_error(log)
                        
        except Exception as e:
            print(f"❌ Console monitoring failed: {e}")
            
    async def _monitor_daemons(self):
        """Monitor daemon status changes"""
        try:
            async for event in self.continuum.daemon_events():
                timestamp = datetime.now().strftime("%H:%M:%S")
                
                print(f"🔧 [{timestamp}] Daemon {event.source}: {event.data.get('status', 'unknown')}")
                
                # Auto-restart failed daemons
                if event.data.get('status') == 'failed':
                    print(f"🔄 Auto-restarting failed daemon: {event.source}")
                    await self.continuum.daemon_restart(event.source)
                    
        except Exception as e:
            print(f"❌ Daemon monitoring failed: {e}")
            
    async def _health_check_loop(self):
        """Periodic health checks"""
        while True:
            try:
                # Get comprehensive status
                status = await self.continuum.status()
                
                if not status.running:
                    print("⚠️ System health check failed - attempting recovery...")
                    # Could trigger auto-recovery here
                    
                # Check daemon health
                daemon_status = await self.continuum.daemon_status()
                running_daemons = sum(1 for d in daemon_status.get('daemons', {}).values() 
                                    if d.get('status') == 'running')
                
                print(f"💚 Health: {running_daemons} daemons running")
                
                await asyncio.sleep(30)  # Check every 30 seconds
                
            except Exception as e:
                print(f"❌ Health check failed: {e}")
                await asyncio.sleep(60)  # Longer wait on failure
                
    async def _command_interface(self):
        """Interactive command interface"""
        print("\n🎮 AI Portal Command Interface")
        print("Commands: health, screenshot, restart <daemon>, deploy, quit")
        
        while True:
            try:
                # In real implementation, this would be async input
                await asyncio.sleep(0.1)
                
                # Simulate some commands for demo
                await asyncio.sleep(10)
                await self._demo_commands()
                
            except KeyboardInterrupt:
                print("\n👋 Shutting down AI Portal...")
                break
                
    async def _demo_commands(self):
        """Demo some portal commands"""
        print("\n🎯 Demo: Executing portal commands...")
        
        # Health check
        health = await self.continuum.health()
        print(f"  ✅ Health: {health}")
        
        # Screenshot for visual validation
        screenshot = await self.continuum.browser_screenshot()
        if screenshot:
            print(f"  📸 Screenshot captured: {len(screenshot)} bytes")
            # Could save to file for AI analysis
        
        # Check project status (dynamic command discovery)
        try:
            projects = await self.continuum.projects_list()
            print(f"  📋 Projects: {projects}")
        except Exception as e:
            print(f"  ⚠️ Projects command not available: {e}")
            
        # JTAG daemon control
        daemons = await self.continuum.daemon_list()
        print(f"  🔧 Active daemons: {len(daemons)}")
        
    async def _handle_error(self, log_event):
        """AI-driven error handling"""
        error_msg = log_event.data.get('message', '')
        source = log_event.source
        
        print(f"🤖 AI analyzing error from {source}: {error_msg[:50]}...")
        
        # AI decision logic could go here
        if "connection" in error_msg.lower():
            print("🔄 Connection error detected - checking daemon status...")
            status = await self.continuum.daemon_status()
            # Could restart daemons or fix networking
            
        elif "timeout" in error_msg.lower():
            print("⏱️ Timeout detected - may need to scale resources...")
            # Could trigger performance optimization
            
        # Log for AI learning
        self.session_log.append({
            "time": datetime.now().isoformat(),
            "type": "ai_response", 
            "trigger": error_msg,
            "action": "analyzed"
        })
        
    async def shutdown(self):
        """Clean shutdown"""
        if self.continuum:
            await self.continuum.close()
        
        # Save session log
        with open("ai_portal_session.json", "w") as f:
            json.dump(self.session_log, f, indent=2)
            
        print("💾 Session log saved")

async def main():
    """Main portal execution"""
    portal = AIPortal()
    
    try:
        await portal.start()
    except KeyboardInterrupt:
        print("\n👋 Portal interrupted by user")
    finally:
        await portal.shutdown()

if __name__ == "__main__":
    asyncio.run(main())