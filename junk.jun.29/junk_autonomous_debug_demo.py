#!/usr/bin/env python3
"""
Autonomous Browser Debugging Demo
Proves JTAG capabilities: console monitoring + error detection + visual validation
"""

import asyncio
import json
import base64
from datetime import datetime
from pathlib import Path
from continuum_client.async_client import connect, LogLevel

class AutonomousDebugger:
    """
    AI-powered browser debugger using JTAG hooks
    Monitors console, detects errors, captures visual evidence
    """
    
    def __init__(self):
        self.continuum = None
        self.debug_session = {
            "start_time": datetime.now().isoformat(),
            "errors_detected": [],
            "screenshots_taken": [],
            "actions_performed": []
        }
        self.screenshot_dir = Path("debug_screenshots")
        self.screenshot_dir.mkdir(exist_ok=True)
        
    async def start_debugging(self):
        """Start autonomous debugging session"""
        print("🔍 Autonomous Browser Debugger - Starting JTAG Session")
        print("=" * 60)
        
        # Connect with elegant async client
        self.continuum = await connect()
        print("✅ Connected to Continuum system")
        
        # Initial system status
        await self._baseline_check()
        
        # Start real-time monitoring
        print("\n📡 Starting real-time monitoring...")
        await self._start_monitoring()
        
    async def _baseline_check(self):
        """Establish baseline system state"""
        print("\n📊 Baseline System Check:")
        
        # System health
        try:
            health = await self.continuum.health()
            print(f"  ✅ System health: {health}")
        except Exception as e:
            print(f"  ❌ Health check failed: {e}")
        
        # Daemon status
        try:
            daemons = await self.continuum.daemon_status()
            running_count = sum(1 for d in daemons.get('daemons', {}).values() 
                              if d.get('status') == 'running')
            print(f"  ✅ Daemons running: {running_count}")
        except Exception as e:
            print(f"  ❌ Daemon check failed: {e}")
        
        # Initial screenshot
        try:
            screenshot = await self.continuum.browser_screenshot()
            if screenshot:
                timestamp = datetime.now().strftime("%H%M%S")
                screenshot_path = self.screenshot_dir / f"baseline_{timestamp}.png"
                screenshot_path.write_bytes(screenshot)
                print(f"  📸 Baseline screenshot saved: {screenshot_path}")
                
                self.debug_session["screenshots_taken"].append({
                    "time": timestamp,
                    "type": "baseline",
                    "path": str(screenshot_path),
                    "size": len(screenshot)
                })
            else:
                print("  ⚠️ No screenshot available")
        except Exception as e:
            print(f"  ❌ Screenshot failed: {e}")
    
    async def _start_monitoring(self):
        """Start comprehensive real-time monitoring"""
        monitoring_tasks = [
            asyncio.create_task(self._monitor_console_errors()),
            asyncio.create_task(self._monitor_daemon_health()),
            asyncio.create_task(self._periodic_health_check()),
            asyncio.create_task(self._demo_error_simulation())
        ]
        
        try:
            await asyncio.gather(*monitoring_tasks, return_exceptions=True)
        except KeyboardInterrupt:
            print("\n⏹️ Debugging session interrupted by user")
        finally:
            await self._save_debug_report()
    
    async def _monitor_console_errors(self):
        """Monitor browser console for errors in real-time"""
        print("🚨 Monitoring console for errors...")
        
        error_count = 0
        try:
            async for log in self.continuum.console_stream():
                timestamp = datetime.now().strftime("%H:%M:%S")
                
                # Focus on errors and warnings
                if log.level in ["error", "warn"]:
                    error_count += 1
                    
                    error_data = {
                        "time": timestamp,
                        "level": log.level,
                        "source": log.source,
                        "message": log.data.get('message', ''),
                        "stack_trace": log.data.get('stackTrace', ''),
                        "url": log.data.get('url', ''),
                        "line": log.data.get('line', '')
                    }
                    
                    self.debug_session["errors_detected"].append(error_data)
                    
                    # Visual error reporting
                    level_emoji = "❌" if log.level == "error" else "⚠️"
                    print(f"\n{level_emoji} [{timestamp}] Console {log.level.upper()}")
                    print(f"  Source: {log.source}")
                    print(f"  Message: {log.data.get('message', '')[:100]}...")
                    
                    if log.data.get('url'):
                        print(f"  Location: {log.data.get('url')}:{log.data.get('line', '?')}")
                    
                    # AUTONOMOUS RESPONSE: Take screenshot on error
                    if log.level == "error":
                        await self._respond_to_error(error_data)
                
                # Progress indicator for info logs
                elif log.level == "info":
                    print(".", end="", flush=True)
                
                # Limit for demo
                if error_count >= 5:
                    print(f"\n📊 Captured {error_count} errors for analysis")
                    break
                    
        except Exception as e:
            print(f"\n❌ Console monitoring failed: {e}")
    
    async def _respond_to_error(self, error_data):
        """Autonomous response to detected errors"""
        timestamp = datetime.now().strftime("%H%M%S")
        
        print(f"  🤖 AUTONOMOUS RESPONSE: Error detected at {timestamp}")
        
        # 1. Capture visual evidence
        try:
            screenshot = await self.continuum.browser_screenshot()
            if screenshot:
                screenshot_path = self.screenshot_dir / f"error_{timestamp}_{error_data['source']}.png"
                screenshot_path.write_bytes(screenshot)
                print(f"  📸 Error screenshot saved: {screenshot_path}")
                
                self.debug_session["screenshots_taken"].append({
                    "time": timestamp,
                    "type": "error_response",
                    "error_source": error_data['source'],
                    "path": str(screenshot_path),
                    "size": len(screenshot)
                })
        except Exception as e:
            print(f"  ❌ Screenshot capture failed: {e}")
        
        # 2. Check daemon health
        try:
            daemon_status = await self.continuum.daemon_status()
            print(f"  🔧 Daemon health check: {len(daemon_status.get('daemons', {}))} daemons")
        except Exception as e:
            print(f"  ❌ Daemon check failed: {e}")
        
        # 3. Log autonomous action
        action = {
            "time": timestamp,
            "trigger": f"{error_data['level']} in {error_data['source']}",
            "actions": ["screenshot_captured", "daemon_health_checked"],
            "error_message": error_data['message'][:100]
        }
        self.debug_session["actions_performed"].append(action)
        
        # 4. AI Analysis (simplified for demo)
        await self._analyze_error_pattern(error_data)
    
    async def _analyze_error_pattern(self, error_data):
        """AI-powered error pattern analysis"""
        message = error_data['message'].lower()
        
        print(f"  🧠 AI Analysis:")
        
        # Pattern recognition
        if "network" in message or "fetch" in message or "xhr" in message:
            print("    📡 Network-related error detected")
            print("    💡 Suggestion: Check API endpoints and connectivity")
            
        elif "undefined" in message or "null" in message:
            print("    🔍 Data validation error detected")
            print("    💡 Suggestion: Check data flow and initialization")
            
        elif "syntax" in message or "unexpected" in message:
            print("    📝 Syntax/parsing error detected")
            print("    💡 Suggestion: Check recent code changes")
            
        elif "timeout" in message:
            print("    ⏱️ Timeout error detected")
            print("    💡 Suggestion: Check performance and resource usage")
            
        else:
            print("    ❓ Unknown error pattern")
            print("    💡 Suggestion: Manual investigation needed")
    
    async def _monitor_daemon_health(self):
        """Monitor daemon health changes"""
        try:
            async for event in self.continuum.daemon_events():
                timestamp = datetime.now().strftime("%H:%M:%S")
                
                status = event.data.get('status', 'unknown')
                daemon_name = event.source
                
                if status in ['failed', 'crashed', 'stopped']:
                    print(f"\n🔧 [{timestamp}] Daemon {daemon_name}: {status}")
                    
                    # Autonomous daemon restart
                    if status in ['failed', 'crashed']:
                        print(f"  🔄 Auto-restarting {daemon_name}...")
                        try:
                            await self.continuum.daemon_restart(daemon_name)
                            print(f"  ✅ {daemon_name} restart initiated")
                        except Exception as e:
                            print(f"  ❌ Restart failed: {e}")
                            
        except Exception as e:
            print(f"❌ Daemon monitoring failed: {e}")
    
    async def _periodic_health_check(self):
        """Periodic comprehensive health checks"""
        while True:
            try:
                await asyncio.sleep(15)  # Check every 15 seconds
                
                timestamp = datetime.now().strftime("%H:%M:%S")
                print(f"\n💚 [{timestamp}] Periodic health check...")
                
                # System status
                status = await self.continuum.status()
                if not status.running:
                    print("  ⚠️ System unhealthy detected!")
                    
                    # Take diagnostic screenshot
                    screenshot = await self.continuum.browser_screenshot()
                    if screenshot:
                        screenshot_path = self.screenshot_dir / f"diagnostic_{timestamp.replace(':', '')}.png"
                        screenshot_path.write_bytes(screenshot)
                        print(f"  📸 Diagnostic screenshot: {screenshot_path}")
                
            except Exception as e:
                print(f"  ❌ Health check failed: {e}")
    
    async def _demo_error_simulation(self):
        """Simulate some errors for demo purposes"""
        await asyncio.sleep(5)  # Let monitoring start
        
        print("\n🎭 Demo: Simulating browser interactions...")
        
        try:
            # Try to navigate (might cause errors if browser not ready)
            await self.continuum.browser_navigate("http://localhost:9000")
            print("  ✅ Navigation command sent")
            
            await asyncio.sleep(3)
            
            # Try some commands that might fail
            try:
                await self.continuum.nonexistent_command()
            except Exception:
                print("  📝 Demo: Triggered command error (expected)")
            
        except Exception as e:
            print(f"  📝 Demo simulation: {e}")
    
    async def _save_debug_report(self):
        """Save comprehensive debug report"""
        self.debug_session["end_time"] = datetime.now().isoformat()
        self.debug_session["duration_minutes"] = (
            datetime.fromisoformat(self.debug_session["end_time"]) - 
            datetime.fromisoformat(self.debug_session["start_time"])
        ).total_seconds() / 60
        
        report_path = Path("debug_report.json")
        with open(report_path, "w") as f:
            json.dump(self.debug_session, f, indent=2)
        
        print(f"\n📋 Debug Report Saved: {report_path}")
        print("\n📊 SESSION SUMMARY:")
        print(f"  • Duration: {self.debug_session['duration_minutes']:.1f} minutes")
        print(f"  • Errors detected: {len(self.debug_session['errors_detected'])}")
        print(f"  • Screenshots taken: {len(self.debug_session['screenshots_taken'])}")
        print(f"  • Autonomous actions: {len(self.debug_session['actions_performed'])}")
        
        if self.debug_session['screenshots_taken']:
            print(f"\n📸 Screenshots saved in: {self.screenshot_dir}")
            for shot in self.debug_session['screenshots_taken']:
                print(f"  • {shot['type']}: {shot['path']}")

async def main():
    """Main debugging demonstration"""
    debugger = AutonomousDebugger()
    
    try:
        await debugger.start_debugging()
    except KeyboardInterrupt:
        print("\n👋 Debug session ended by user")
    finally:
        if debugger.continuum:
            await debugger.continuum.close()

if __name__ == "__main__":
    print("🚀 Starting Autonomous Browser Debugging Demo")
    print("This proves JTAG capabilities: real-time console monitoring + visual validation")
    print("Press Ctrl+C to end the session")
    print()
    
    asyncio.run(main())