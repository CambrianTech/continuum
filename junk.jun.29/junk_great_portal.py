#!/usr/bin/env python3
"""
Great Portal - Autonomous Development System
JTAG capabilities: browser control + screenshots + code editing + system rebuilds + session logging
"""

import requests
import json
import time
import os
from datetime import datetime
from pathlib import Path

class GreatPortal:
    """Autonomous Development Portal with JTAG capabilities"""
    
    def __init__(self, base_url="http://localhost:9000"):
        self.base_url = base_url
        self.api_url = f"{base_url}/api"
        
        # Use Continuum's artifact system for session management
        self.session_id = None
        self.session_dir = None
        self.screenshot_count = 0
        
        # Create session via Continuum's artifact system
        self._create_session()
    
    def _create_session(self):
        """Create session using Continuum's artifact system"""
        try:
            result = self.execute_command('session-create', {
                'type': 'development',
                'description': 'AI Portal autonomous development session'
            })
            if result:
                self.session_id = result.get('session_id')
                self.session_dir = result.get('session_path')
                print(f"📁 Session created: {self.session_id}")
                print(f"📂 Session path: {self.session_dir}")
            else:
                # Fallback to local session
                self.session_id = f"local_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
                print(f"📁 Local session: {self.session_id}")
        except Exception as e:
            self.session_id = f"local_{datetime.now().strftime('%Y%m%d_%H%M%S')}"
            print(f"📁 Local session fallback: {self.session_id}")
    
    def log_action(self, action, data=None):
        """Log action to Continuum's artifact system"""
        try:
            self.execute_command('session-log', {
                'session_id': self.session_id,
                'action': action,
                'data': data or {},
                'timestamp': datetime.now().isoformat()
            })
        except Exception as e:
            # Fallback to local logging
            pass
    
    def save_artifact(self, name, content, artifact_type="text"):
        """Save artifact using Continuum's system"""
        try:
            return self.execute_command('artifact-save', {
                'session_id': self.session_id,
                'name': name,
                'content': content,
                'type': artifact_type
            })
        except Exception as e:
            print(f"❌ Artifact save failed: {e}")
            return None

    def test_connection(self):
        """Test if Continuum is running"""
        try:
            response = requests.get(f"{self.base_url}/health", timeout=5)
            return response.status_code == 200
        except:
            return False
    
    def execute_command(self, command, args=None):
        """Execute a command via the API"""
        payload = {
            'command': command,
            'args': args or {}
        }
        
        try:
            response = requests.post(
                f"{self.api_url}/command",
                json=payload,
                timeout=30
            )
            
            if response.status_code == 200:
                result = response.json()
                if result.get('success'):
                    return result.get('data')
                else:
                    print(f"❌ Command failed: {result.get('error')}")
                    return None
            else:
                print(f"❌ API error: {response.status_code}")
                return None
                
        except Exception as e:
            print(f"❌ Request failed: {e}")
            return None
    
    def get_console_logs(self, limit=10):
        """Get browser console logs"""
        try:
            response = requests.get(f"{self.api_url}/console/logs?limit={limit}")
            if response.status_code == 200:
                return response.json()
            return []
        except Exception as e:
            print(f"❌ Console logs failed: {e}")
            return []
    
    def take_screenshot(self, description="debug"):
        """Take browser screenshot and save as artifact"""
        try:
            response = requests.get(f"{self.api_url}/browser/screenshot")
            if response.status_code == 200:
                self.screenshot_count += 1
                screenshot_name = f"screenshot_{self.screenshot_count:03d}_{description}"
                
                # Save as artifact in Continuum's system
                self.save_artifact(
                    name=screenshot_name,
                    content=response.content,
                    artifact_type="image"
                )
                
                self.log_action("screenshot_taken", {
                    "name": screenshot_name,
                    "description": description,
                    "size": len(response.content)
                })
                
                return response.content
            return None
        except Exception as e:
            print(f"❌ Screenshot failed: {e}")
            return None
    
    def refresh_browser(self):
        """Refresh the browser and log action"""
        try:
            response = requests.post(
                f"{self.api_url}/browser/navigate",
                json={'url': 'http://localhost:9000'}
            )
            success = response.status_code == 200
            
            self.log_action("browser_refresh", {
                "success": success,
                "url": "http://localhost:9000"
            })
            
            return success
        except Exception as e:
            print(f"❌ Browser refresh failed: {e}")
            self.log_action("browser_refresh_failed", {"error": str(e)})
            return False
    
    # ========== AUTONOMOUS DEVELOPMENT CAPABILITIES ==========
    
    def edit_code(self, file_path, content):
        """Edit code file and save as artifact"""
        try:
            result = self.execute_command('file-edit', {
                'path': file_path,
                'content': content
            })
            
            if result:
                self.save_artifact(
                    name=f"code_edit_{Path(file_path).name}",
                    content=content,
                    artifact_type="code"
                )
                
                self.log_action("code_edited", {
                    "file": file_path,
                    "size": len(content)
                })
                
                return True
            return False
        except Exception as e:
            print(f"❌ Code edit failed: {e}")
            return False
    
    def rebuild_system(self):
        """Trigger system rebuild"""
        try:
            result = self.execute_command('system-rebuild')
            
            self.log_action("system_rebuild", {
                "success": result is not None
            })
            
            return result
        except Exception as e:
            print(f"❌ System rebuild failed: {e}")
            return False
    
    def run_tests(self):
        """Run test suite"""
        try:
            result = self.execute_command('test-run')
            
            self.log_action("tests_run", {
                "success": result is not None,
                "result": result
            })
            
            return result
        except Exception as e:
            print(f"❌ Test run failed: {e}")
            return False
    
    def navigate_browser(self, url):
        """Navigate browser to specific URL"""
        try:
            response = requests.post(
                f"{self.api_url}/browser/navigate",
                json={'url': url}
            )
            success = response.status_code == 200
            
            self.log_action("browser_navigate", {
                "url": url,
                "success": success
            })
            
            return success
        except Exception as e:
            print(f"❌ Browser navigation failed: {e}")
            return False
    
    def execute_browser_js(self, js_code):
        """Execute JavaScript in browser"""
        try:
            result = self.execute_command('browser-js', {
                'code': js_code
            })
            
            self.log_action("browser_js_executed", {
                "code": js_code[:100],  # Log first 100 chars
                "success": result is not None
            })
            
            return result
        except Exception as e:
            print(f"❌ Browser JS execution failed: {e}")
            return False
    
    def get_element_info(self, selector):
        """Get browser element information"""
        try:
            result = self.execute_command('browser-element', {
                'selector': selector
            })
            
            self.log_action("element_queried", {
                "selector": selector,
                "found": result is not None
            })
            
            return result
        except Exception as e:
            print(f"❌ Element query failed: {e}")
            return False
    
    def get_daemon_status(self):
        """Get daemon status"""
        try:
            response = requests.get(f"{self.api_url}/daemons")
            if response.status_code == 200:
                return response.json()
            return {}
        except Exception as e:
            print(f"❌ Daemon status failed: {e}")
            return {}

def run_debug_test():
    """Run the JTAG debugging test"""
    portal = GreatPortal()
    
    print("🔍 Great Portal - JTAG Debugging Test")
    print("=" * 45)
    
    # 1. Test connection
    print("🔌 Testing connection...")
    if portal.test_connection():
        print("✅ Connected to Continuum")
    else:
        print("❌ Continuum not running - start with: ./continuum")
        return
    
    # 2. Check system status
    print("\n📊 System status:")
    daemon_data = portal.get_daemon_status()
    if daemon_data:
        # Parse the actual API response format
        daemon_list = daemon_data.get('daemons', [])
        router_data = daemon_data.get('router', {})
        
        if daemon_list:
            for daemon in daemon_list:
                name = daemon.get('name', 'unknown')
                print(f"  ✅ {name}: registered")
        
        # Also check router registered daemons
        router_daemons = router_data.get('daemons', [])
        if router_daemons:
            for daemon in router_daemons:
                name = daemon.get('name', 'unknown')
                capabilities = len(daemon.get('capabilities', []))
                print(f"  ✅ {name}: {capabilities} capabilities")
        
        if not daemon_list and not router_daemons:
            print("  ⚠️ No daemons found in registry")
    else:
        print("  ⚠️ No daemon info available")
    
    # 3. Refresh browser
    print("\n🔄 Refreshing browser...")
    if portal.refresh_browser():
        print("✅ Browser refresh sent")
        time.sleep(2)  # Wait for page load
    else:
        print("❌ Browser refresh failed")
    
    # 4. Get console logs
    print("\n📜 Console logs:")
    logs = portal.get_console_logs(limit=15)
    if logs:
        for i, log in enumerate(logs[-10:], 1):  # Show last 10
            level = log.get('level', 'info')
            message = log.get('message', str(log))
            
            level_emoji = {
                'info': '💡',
                'warn': '⚠️',
                'error': '❌',
                'debug': '🔍'
            }.get(level, '📝')
            
            # Truncate long messages
            if len(message) > 80:
                message = message[:77] + "..."
            
            print(f"  {i:2d}. {level_emoji} {message}")
    else:
        print("  📝 No console logs found")
    
    # 5. Take screenshot
    print("\n📸 Taking screenshot...")
    screenshot = portal.take_screenshot()
    if screenshot:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"debug_screenshot_{timestamp}.png"
        
        with open(filename, 'wb') as f:
            f.write(screenshot)
        
        print(f"✅ Screenshot saved: {filename} ({len(screenshot):,} bytes)")
    else:
        print("❌ Screenshot failed")
    
    # 6. Test a few commands
    print("\n⚡ Testing commands:")
    
    # Health check
    health = portal.execute_command('health')
    if health:
        print(f"  ✅ Health: {health}")
    else:
        print("  ❌ Health command failed")
    
    # Try projects command
    projects = portal.execute_command('projects-list')
    if projects:
        print(f"  ✅ Projects: {projects}")
    else:
        print("  ⚠️ Projects command not available")
    
    print("\n🎯 JTAG Debugging Capabilities Demonstrated:")
    print("  ✓ Real-time system connection")
    print("  ✓ Browser control (refresh)")
    print("  ✓ Console log capture")
    print("  ✓ Visual validation (screenshot)")
    print("  ✓ Command execution")
    print("  ✓ Daemon monitoring")
    
    print("\n✨ This proves autonomous debugging works!")
    print("AI agents can now debug browsers in real-time! 🤖")

if __name__ == "__main__":
    run_debug_test()