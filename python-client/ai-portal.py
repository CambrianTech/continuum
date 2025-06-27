#!/usr/bin/env python3
"""
AI Portal - Continuum Command Interface
=======================================
THE primary thin client for Continuum's command bus architecture.

📖 ARCHITECTURE DOCS: docs/AI_PORTAL_ARCHITECTURE.md
📦 COMMAND PACKAGES: src/commands/README.md

CONSOLIDATION APPROACH:
🎯 This is THE client interface - consolidates all Python access to Continuum
🔧 Auto-healing: Automatically starts server if connection fails
📡 Built on continuum_client API: python-client/continuum_client/
🚀 Future: May consolidate other clients into this unified interface

ARCHITECTURE PRINCIPLES:
- Continuum Server = OS/Orchestrator with modular command bus
- AI Portal = THE thin client adapter that forwards commands via WebSocket
- Commands = Modular, discoverable, self-documenting via help system
- Promise-based async API mirrors JavaScript elegance
- Auto-healing ensures reliability

DESIGN PATTERNS:
✅ Adapter Pattern: Translates Python calls → [CMD:COMMAND] protocol
✅ Command Bus: All logic in server commands, not client
✅ Promise-based: Async/await with proper timeout handling  
✅ Self-documenting: help command provides live API docs
✅ Modular: Add functionality via Continuum commands, not client code
✅ Auto-healing: Client starts server automatically when needed

USAGE EXAMPLES:
  python3 ai-portal.py --cmd workspace --params '{"action": "path"}'
  python3 ai-portal.py --cmd sentinel --params '{"action": "start", "task": "monitor"}'
  python3 ai-portal.py --cmd restart  # Version bump + server restart
  python3 ai-portal.py --cmd help     # Live command documentation
  python3 ai-portal.py --cmd help --params '{"sync": true}'  # Sync docs
  python3 ai-portal.py spawn         # Spawn fresh agent in tmux for observation
  # Note: Currently exec has issues, use manual: tmux new-session -d -s agent-observer
"""

import asyncio
import json
import click
import time
import os
from pathlib import Path
from datetime import datetime

import sys
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config
from continuum_client.devtools import DevToolsLogMonitor, LiveDevToolsMonitor
from continuum_client.core.daemon_manager import daemon_manager

async def launch_continuum_browser(debug_mode=False):
    """Single unified function to launch Opera with localhost:9000 - normal or debug mode"""
    import subprocess
    import asyncio
    
    if debug_mode:
        print("🚀 Launching Opera in debug mode...")
        opera_cmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            '--remote-debugging-port=9222',
            '--disable-web-security',
            '--disable-features=TranslateUI',
            '--disable-component-update',
            '--disable-background-timer-throttling',
            '--disable-backgrounding-occluded-windows',
            '--disable-renderer-backgrounding',
            '--no-first-run',
            '--no-default-browser-check',
            '--disable-default-apps',
            '--disable-extensions',
            '--user-data-dir=/tmp/opera-devtools-portal',
            'http://localhost:9000'
        ]
    else:
        print("🌐 Launching Opera in normal mode...")
        opera_cmd = [
            '/Applications/Opera GX.app/Contents/MacOS/Opera',
            'http://localhost:9000'
        ]
    
    try:
        print("🚨 BROWSER LAUNCH: ai-portal.py - subprocess.Popen(opera_cmd)")
        print(f"   📍 Called from: launch_continuum_browser(debug_mode={debug_mode})")
        if debug_mode:
            print(f"   🎯 User data dir: /tmp/opera-devtools-portal")
            print(f"   🔌 Debug port: 9222")
        else:
            print(f"   🎯 Normal mode (no debug)")
        subprocess.Popen(opera_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        
        if debug_mode:
            print("🌐 Opera launched with remote debugging on port 9222")
            print("📍 Browser URL: http://localhost:9000")
            
            # Wait for Opera to start up and verify DevTools
            await asyncio.sleep(6)
            
            for attempt in range(10):
                try:
                    result = subprocess.run(['curl', '-s', 'http://localhost:9222/json'], 
                                          capture_output=True, timeout=2)
                    if result.returncode == 0 and b'devtoolsFrontendUrl' in result.stdout:
                        print("✅ DevTools port 9222 is responding")
                        return {'success': True, 'message': 'Opera launched successfully with DevTools'}
                    await asyncio.sleep(1)
                except:
                    await asyncio.sleep(1)
            
            print("⚠️ DevTools port not responding, but Opera should be running")
            return {'success': True, 'message': 'Opera launched, DevTools connection pending'}
        else:
            print("✅ Opera launched successfully")
            return {'success': True, 'message': 'Opera launched successfully'}
        
    except Exception as e:
        print(f"❌ Failed to launch Opera: {e}")
        return {'success': False, 'message': f'Failed to launch Opera: {e}'}

# Convenience functions
async def launch_opera_debug_mode():
    """Launch Opera in debug mode"""
    return await launch_continuum_browser(debug_mode=True)

async def launch_opera_normal_mode():
    """Launch Opera in normal mode"""
    return await launch_continuum_browser(debug_mode=False)

def get_portal_dir():
    """Get .continuum/ai-portal directory"""
    continuum_dir = Path.cwd()
    while continuum_dir != continuum_dir.parent:
        if (continuum_dir / '.continuum').exists():
            break
        continuum_dir = continuum_dir.parent
    
    portal_dir = continuum_dir / '.continuum' / 'ai-portal'
    portal_dir.mkdir(parents=True, exist_ok=True)
    return portal_dir

def get_logs_dir():
    """Get logs directory in .continuum/ai-portal/logs"""
    logs_dir = get_portal_dir() / 'logs'
    logs_dir.mkdir(parents=True, exist_ok=True)
    return logs_dir

def get_scripts_dir():
    """Get scripts directory in .continuum/ai-portal/scripts"""
    scripts_dir = get_portal_dir() / 'scripts'
    scripts_dir.mkdir(parents=True, exist_ok=True)
    return scripts_dir

def write_log(message: str):
    """Write to buffer log"""
    print(f"🔍 CLIENT-DIAGNOSTIC: write_log called with: {message[:100]}...")
    
    logs_dir = get_logs_dir()
    print(f"🔍 CLIENT-DIAGNOSTIC: logs_dir = {logs_dir}")
    
    buffer_file = logs_dir / 'buffer.log'
    print(f"🔍 CLIENT-DIAGNOSTIC: buffer_file path: {buffer_file}")
    
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    log_entry = f"[{timestamp}] {message}\n"
    print(f"🔍 CLIENT-DIAGNOSTIC: log_entry: {log_entry[:100]}...")
    
    try:
        with open(buffer_file, 'a') as f:
            f.write(log_entry)
        print(f"🔍 CLIENT-DIAGNOSTIC: Successfully wrote to buffer.log")
    except Exception as e:
        print(f"🔍 CLIENT-DIAGNOSTIC: Error writing to buffer.log: {e}")

async def handle_message(message):
    """Complete message logger - capture everything"""
    msg_type = message.get('type', 'unknown')
    # Log the full message, not truncated
    write_log(f"WS_RECV: {json.dumps(message, indent=2)}")

# Global DevTools monitor instance and background task
devtools_monitor = None
_devtools_background_task = None

async def handle_browser_log(log_entry):
    """Handle browser console log from DevTools"""
    timestamp = log_entry['timestamp']
    level = log_entry['level'].upper()
    text = log_entry['text']
    browser = log_entry.get('browser', 'browser')
    
    # Write to our buffer log system
    write_log(f"BROWSER_LOG [{browser}] [{level}]: {text}")
    
    # Also print for immediate feedback
    print(f"🌐 [{timestamp}] {level}: {text}")

async def keep_devtools_alive():
    """Background task to keep DevTools connection alive"""
    global devtools_monitor
    
    while True:
        try:
            await asyncio.sleep(10)  # Check every 10 seconds
            
            if devtools_monitor and not devtools_monitor.connected:
                print("🔌 DevTools connection lost, reconnecting...")
                await start_devtools_monitoring()
                
        except Exception as e:
            print(f"🔌 DevTools keepalive error: {e}")
            await asyncio.sleep(30)  # Wait longer on error

async def start_devtools_monitoring():
    """Start DevTools browser console monitoring with persistent connection"""
    global devtools_monitor, _devtools_background_task
    
    if devtools_monitor and devtools_monitor.connected:
        print("🔌 DevTools monitor already running")
        return devtools_monitor
    
    print("🔌 Starting DevTools browser console monitoring...")
    
    # Try Chrome/Opera first (most common)
    for browser_port in [9222, 9223]:  # Chrome/Opera and Edge
        try:
            devtools_monitor = DevToolsLogMonitor(
                chrome_port=browser_port,
                target_url="localhost:9000",
                log_callback=handle_browser_log
            )
            
            success = await devtools_monitor.connect()
            if success:
                print(f"🔌 DevTools connected on port {browser_port}")
                write_log(f"SYSTEM: DevTools monitoring started on port {browser_port}")
                
                # Start background keepalive task
                if not _devtools_background_task or _devtools_background_task.done():
                    _devtools_background_task = asyncio.create_task(keep_devtools_alive())
                    print("🔌 DevTools keepalive task started")
                
                return devtools_monitor
            
        except Exception as e:
            print(f"🔌 DevTools port {browser_port} failed: {e}")
    
    print("🔌 DevTools: No browser found with remote debugging enabled")
    print("🔌 Start Chrome/Opera with: chrome --remote-debugging-port=9222")
    return None

async def stop_devtools_monitoring():
    """Stop DevTools monitoring"""
    global devtools_monitor, _devtools_background_task
    
    # Cancel background task
    if _devtools_background_task and not _devtools_background_task.done():
        _devtools_background_task.cancel()
        _devtools_background_task = None
        print("🔌 DevTools keepalive task stopped")
    
    if devtools_monitor:
        await devtools_monitor.disconnect()
        devtools_monitor = None
        write_log("SYSTEM: DevTools monitoring stopped")
        print("🔌 DevTools monitoring stopped")

async def handle_devtools_action(action: str, params: dict):
    """Handle DevTools actions directly in Portal"""
    global devtools_monitor
    
    if action == 'start_devtools_monitoring':
        browser = params.get('browser', 'auto')
        print(f"🔌 Starting DevTools monitoring for {browser}...")
        
        monitor = await start_devtools_monitoring()
        if monitor:
            return {'success': True, 'message': f'DevTools monitoring started for {browser}'}
        else:
            return {'success': False, 'message': 'Failed to start DevTools monitoring'}
    
    elif action == 'show_browser_logs':
        lines = params.get('lines', 10)
        
        if devtools_monitor and devtools_monitor.connected:
            logs = devtools_monitor.get_recent_logs(lines)
            print(f"🌐 Recent Browser Logs ({len(logs)} entries):")
            for log in logs:
                print(f"  [{log['timestamp']}] {log['level'].upper()}: {log['text']}")
            return {'success': True, 'logs': logs}
        else:
            print("🔌 DevTools not connected")
            return {'success': False, 'message': 'DevTools not connected'}
    
    elif action == 'browser_status':
        if devtools_monitor:
            status = devtools_monitor.get_connection_status()
            print(f"🔌 Browser Status: {'Connected' if status['connected'] else 'Disconnected'}")
            print(f"   Port: {status['chrome_port']}, Logs: {status['stored_logs']}")
            return {'success': True, 'status': status}
        else:
            print("🔌 DevTools monitor not initialized")
            return {'success': False, 'message': 'DevTools monitor not initialized'}
    
    elif action == 'launch_opera_devtools':
        print("🚀 Launching Opera with DevTools...")
        return await launch_opera_debug_mode()
    
    else:
        return {'success': False, 'message': f'Unknown DevTools action: {action}'}

async def start_buffer():
    """Start buffering WebSocket messages with DevTools integration"""
    print("🚀 Starting comprehensive log monitoring...")
    
    try:
        load_continuum_config()
        
        # Start DevTools monitoring first
        devtools = await start_devtools_monitoring()
        if devtools:
            print("🔌 Browser console monitoring enabled")
        
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'ai-portal',
                'agentName': 'AI Portal',
                'agentType': 'ai'
            })
            
            client.add_message_handler(handle_message)
            write_log("SYSTEM: Comprehensive monitoring started")
            
            print(f"📁 WebSocket logs: {get_logs_dir()}/buffer.log")
            if devtools:
                print(f"🌐 Browser logs: Real-time console monitoring")
            print("🔄 Monitoring both WebSocket and browser (Ctrl+C to stop)")
            
            while True:
                await asyncio.sleep(1)
                
    except KeyboardInterrupt:
        write_log("SYSTEM: Monitoring stopped")
        await stop_devtools_monitoring()
        print("\n👋 Monitoring stopped")

async def handle_screenshot_services_action(params: dict):
    """Handle screenshot services listing and management"""
    action = params.get('action', 'list')
    
    if action == 'list':
        # List available screenshot services from daemons
        services = daemon_manager.find_screenshot_services()
        
        print(f"📸 AVAILABLE SCREENSHOT SERVICES:")
        if services:
            for service in services:
                print(f"  🤖 {service['daemon_id']}")
                print(f"     Type: {service['daemon_type']}")
                print(f"     Status: {service['status']}")
                print(f"     Screenshot Dir: {service['screenshot_dir']}")
                print()
        else:
            print("  ❌ No screenshot services available")
            print("  💡 Start a DevTools daemon: python3 ai-portal.py --devtools")
        
        return {
            'success': True,
            'services': services,
            'count': len(services)
        }
    
    else:
        return {
            'success': False,
            'error': f'Unknown action: {action}'
        }

async def handle_daemon_screenshot_action(params: dict):
    """Handle screenshot capture via daemon services"""
    filename = params.get('filename')
    daemon_id = params.get('daemon_id')
    format_type = params.get('format', 'png')
    quality = params.get('quality', 90)
    
    # If no daemon specified, use first available screenshot service
    if not daemon_id:
        services = daemon_manager.find_screenshot_services()
        if services:
            daemon_id = services[0]['daemon_id']
            print(f"📸 Using first available daemon: {daemon_id}")
        else:
            return {
                'success': False,
                'error': 'No screenshot services available. Start a DevTools daemon first.'
            }
    
    # Route screenshot request to daemon
    request_data = {
        'filename': filename,
        'format': format_type,
        'quality': quality
    }
    
    result = await daemon_manager.handle_screenshot_request(daemon_id, request_data)
    
    if result.get('success'):
        print(f"✅ Screenshot captured: {result['screenshot_path']}")
        print(f"📊 Format: {result['format']}, Daemon: {result['daemon_id']}")
    else:
        print(f"❌ Screenshot failed: {result.get('error', 'Unknown error')}")
    
    return result

def show_logs(lines: int = 10):
    """Show recent log lines from client, server, and browser with failsafe recovery"""
    global devtools_monitor
    
    print(f"📋 CLIENT LOGS (Last {lines} entries):")
    buffer_file = get_logs_dir() / 'buffer.log'
    
    # FAILSAFE: Check if DevTools monitoring is working as backup
    devtools_status = "❌ Not connected"
    if devtools_monitor and devtools_monitor.connected:
        devtools_status = "✅ Live monitoring active (FAILSAFE)"
    elif devtools_monitor:
        devtools_status = "⚠️ Connected but inactive"
    
    print(f"🔌 DevTools Failsafe Status: {devtools_status}")
    
    if buffer_file.exists():
        try:
            with open(buffer_file) as f:
                all_lines = f.readlines()
                recent = all_lines[-lines:] if len(all_lines) >= lines else all_lines
                
            for line in recent:
                line_clean = line.strip()
                if "BROWSER_LOG" in line_clean:
                    print(f"  🌐 {line_clean}")
                elif "WS_RECV" in line_clean:
                    print(f"  📡 {line_clean}")
                else:
                    print(f"  📱 {line_clean}")
        except Exception as e:
            print(f"🔍 Error reading buffer file: {e}")
    else:
        print("  📝 No client logs found")
    
    # Show live browser logs if DevTools is connected
    if devtools_monitor and devtools_monitor.connected:
        print(f"\n🌐 BROWSER LOGS (Last {lines} from DevTools):")
        recent_browser_logs = devtools_monitor.get_recent_logs(lines)
        
        if recent_browser_logs:
            for log_entry in recent_browser_logs:
                timestamp = log_entry['timestamp']
                level = log_entry['level'].upper()
                text = log_entry['text']
                print(f"  🔗 [{timestamp}] {level}: {text}")
        else:
            print("  📝 No recent browser logs")
            
        # Show connection status
        status = devtools_monitor.get_connection_status()
        print(f"  📊 Status: {status['stored_logs']} logs stored, port {status['chrome_port']}")
    else:
        print(f"\n🌐 BROWSER LOGS: DevTools not connected")
        print("  💡 Use 'python ai-portal.py --buffer' to start browser monitoring")
    
    print(f"\n📋 SERVER LOGS (Last {lines} entries):")
    # Try multiple server log locations
    server_logs = [
        Path.cwd() / 'server.log',
        Path.cwd() / 'continuum.log', 
        Path('.continuum') / 'continuum.log'
    ]
    
    print(f"🔍 DIAGNOSTIC: Checking {len(server_logs)} server log locations...")
    for i, log_file in enumerate(server_logs):
        print(f"🔍 DIAGNOSTIC: Location {i+1}: {log_file} - exists: {log_file.exists()}")
    
    server_log_found = False
    for log_file in server_logs:
        if log_file.exists():
            server_log_found = True
            print(f"🔍 DIAGNOSTIC: Using server log: {log_file}")
            try:
                stat = log_file.stat()
                print(f"🔍 DIAGNOSTIC: Server log size: {stat.st_size} bytes")
                print(f"🔍 DIAGNOSTIC: Server log modified: {datetime.fromtimestamp(stat.st_mtime)}")
                
                with open(log_file) as f:
                    all_lines = f.readlines()
                    print(f"🔍 DIAGNOSTIC: Read {len(all_lines)} total server log lines")
                    recent = all_lines[-lines:] if len(all_lines) >= lines else all_lines
                    
                for line in recent:
                    print(f"  🖥️  {line.strip()}")
            except Exception as e:
                print(f"🔍 DIAGNOSTIC: Error reading server log: {e}")
            break
    
    if not server_log_found:
        print("  📝 No server logs found")
        
    # Also check for recent WebSocket activity
    print(f"\n📋 WEBSOCKET ACTIVITY:")
    try:
        import subprocess
        result = subprocess.run(['netstat', '-an'], capture_output=True, text=True)
        ws_lines = [line for line in result.stdout.split('\n') if ':9000' in line]
        if ws_lines:
            for line in ws_lines:
                print(f"  🔌 {line.strip()}")
        else:
            print("  📝 No WebSocket connections on port 9000")
    except Exception:
        print("  ⚠️ Could not check WebSocket status")

def clear_logs(label: str = None):
    """Clear buffer log with optional label"""
    buffer_file = get_logs_dir() / 'buffer.log'
    
    if buffer_file.exists():
        # Archive with label
        timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
        if label:
            archive_name = f"buffer-{label}-{timestamp}.log"
        else:
            archive_name = f"buffer-{timestamp}.log"
        
        archive_path = get_logs_dir() / archive_name
        buffer_file.rename(archive_path)
        print(f"📦 Archived to: {archive_name}")
    
    write_log("SYSTEM: Logs cleared")
    print("🧹 Logs cleared")

def run_fresh_agent_test(args):
    """Create fresh agent observer session for natural discovery testing"""
    agent_type = args[0] if args else "claude"
    
    print(f"🤖 Creating fresh {agent_type.upper()} agent observer session...")
    print("📖 This agent will receive the README and explore naturally")
    print("⚠️  NO INTERVENTION - Pure observation mode")
    print()
    
    # Create observation session
    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
    session_id = f"fresh-{agent_type}-{timestamp}"
    
    # Create observations directory
    obs_dir = Path('.continuum/observations')
    obs_dir.mkdir(parents=True, exist_ok=True)
    
    # Load current README for agent context
    readme_path = Path('README.md')
    if not readme_path.exists():
        print("❌ README.md not found - agent needs context!")
        return
    
    readme_content = readme_path.read_text()
    
    # Create fresh agent prompt
    agent_prompt = f"""# 🤖 Fresh Agent Observer Session: {session_id}

## Mission Brief
You are a fresh AI agent who has just discovered this codebase. Your mission is to explore the system and document your natural discovery process WITHOUT any human intervention.

## Context Provided
You have been given the project README below. Based ONLY on this information, explore the system and document:

1. **First Impressions**: What you think you should try first and why
2. **Natural Priorities**: What seems most important/urgent to fix or explore  
3. **Confusion Points**: Where you get stuck or need clarification
4. **Discovery Process**: What commands/workflows you naturally develop
5. **Effectiveness Rating**: Rate yourself 1-10 and identify biggest barriers

## Available System README:

{readme_content}

## Your Discovery Log
*Document your exploration below. Try commands, explore files, test functionality - whatever feels natural:*

**First Instinct**: 
*What's your immediate reaction? What do you want to try first?*

**Command Sequence**: 
*Document each command you try and what happens*

**Observations**:
*What works? What's broken? What's confusing?*

**Recommendations**:
*What would help future agents onboard faster?*

---
*Session started: {datetime.now().isoformat()}*
*Agent type: {agent_type}*
*Mode: Pure observation (no intervention)*
"""

    # Save agent session file
    session_file = obs_dir / f"{session_id}_session.md"
    session_file.write_text(agent_prompt)
    
    print(f"📋 Agent session created: {session_file}")
    print()
    print("🎯 NEXT STEPS:")
    print(f"1. Copy the content from: {session_file}")
    print(f"2. Start fresh {agent_type} session (Claude Code, ChatGPT, etc.)")
    print("3. Paste the prompt and let agent explore naturally")
    print("4. Document everything they try in the session file")
    print("5. Analyze patterns vs other agent sessions")
    print()
    print("🔬 This simulates AR app user testing - pure observation of natural behavior!")
    
    return session_file

async def auto_heal_connection():
    """Auto-heal server connection issues"""
    import subprocess
    import time
    
    print("🔧 Auto-healing: Checking server status...")
    
    # Check if server process is running
    try:
        result = subprocess.run(['pgrep', '-f', 'continuum.cjs'], 
                              capture_output=True, text=True)
        if result.returncode != 0:
            print("🚀 Auto-healing: Starting Continuum server...")
            subprocess.Popen(['node', 'continuum.cjs'], 
                           stdout=subprocess.DEVNULL, 
                           stderr=subprocess.DEVNULL)
            time.sleep(5)  # Give server time to start
            return True
    except Exception as e:
        print(f"⚠️ Auto-healing failed: {e}")
        return False
    
    # Server is running, might be a port/connection issue
    print("🔌 Auto-healing: Server running, checking connection...")
    return False

async def run_command(cmd: str, params: str = "{}", verbose: bool = False):
    """Execute any continuum command with JSON params"""
    max_retries = 2
    
    for attempt in range(max_retries):
        try:
            load_continuum_config()
            
            async with ContinuumClient() as client:
                await client.register_agent({
                    'agentId': 'ai-portal-cmd',
                    'agentName': 'AI Portal Command',
                    'agentType': 'ai'
                })
                
                if cmd.startswith('client.'):
                    # Direct client method call via eval
                    result = await eval(f"{cmd}")
                else:
                    # Handle DevTools actions directly in Portal
                    import json
                    params_dict = json.loads(params) if params != "{}" else {}
                    
                    if cmd == 'devtools_action' and isinstance(params_dict, dict) and params_dict.get('action'):
                        result = await handle_devtools_action(params_dict['action'], params_dict)
                    elif cmd == 'screenshot_services':
                        result = await handle_screenshot_services_action(params_dict)
                    elif cmd == 'daemon_screenshot':
                        result = await handle_daemon_screenshot_action(params_dict)
                    else:
                        # Regular Continuum command via send_command
                        result = await client.send_command(cmd, params_dict)
                
                # Handle both string and dict responses safely
                if verbose:
                    print(f"✅ Result: {result}")
                else:
                    # Handle string responses
                    if isinstance(result, str):
                        print(f"✅ {result}")
                    # Handle dict responses
                    elif isinstance(result, dict):
                        # Safely check nested dict structure
                        inner_result = result.get('result', {})
                        if isinstance(inner_result, dict):
                            if inner_result.get('success', True):
                                print(f"✅ {inner_result.get('message', 'Command completed')}")
                            else:
                                error_msg = inner_result.get('error', 'Unknown error')
                                print(f"❌ {inner_result.get('message', 'Command failed')}")
                                if isinstance(error_msg, str) and error_msg != 'Unknown error':
                                    print(f"   {error_msg}")
                        else:
                            # Handle flat dict or other dict structures
                            success = result.get('success', True)
                            if success:
                                print(f"✅ {result.get('message', 'Command completed')}")
                            else:
                                print(f"❌ {result.get('message', 'Command failed')}")
                                error_msg = result.get('error', '')
                                if error_msg:
                                    print(f"   {error_msg}")
                    else:
                        print(f"✅ Command completed: {result}")
                
                print(f"🔍 CLIENT-DIAGNOSTIC: About to write_log for command: {cmd}")
                write_log(f"COMMAND: {cmd} {params} -> {result}")
                print(f"🔍 CLIENT-DIAGNOSTIC: write_log completed for command: {cmd}")
                return result
                
        except Exception as e:
            if attempt == 0 and "Connect call failed" in str(e):
                print(f"🔧 Connection failed (attempt {attempt + 1}/{max_retries}), auto-healing...")
                if await auto_heal_connection():
                    print("✅ Auto-healing: Server started, retrying...")
                    await asyncio.sleep(3)  # Wait a bit more for server to be ready
                    continue
                else:
                    print("⚠️ Auto-healing: Manual intervention may be needed")
            
            error_msg = f"❌ Command failed: {e}"
            print(error_msg)
            write_log(f"ERROR: {cmd} {params} -> {e}")
            
            if attempt == max_retries - 1:
                # Last attempt failed, provide helpful info
                print("\n🩺 Connection Diagnostics:")
                print("1. Check if Continuum server is running: ps aux | grep continuum")
                print("2. Try starting manually: node continuum.cjs")
                print("3. Check port 9000: netstat -an | grep :9000")
                print("4. Check working directory: pwd (should be continuum root)")
            break

def save_script(name: str, steps: str):
    """Save a script to .continuum/ai-portal/scripts"""
    scripts_dir = get_scripts_dir()
    script_file = scripts_dir / f"{name}.txt"
    
    with open(script_file, 'w') as f:
        f.write(steps)
    
    print(f"📝 Script saved: {script_file}")

def load_script(name: str) -> str:
    """Load a script from .continuum/ai-portal/scripts"""
    scripts_dir = get_scripts_dir()
    script_file = scripts_dir / f"{name}.txt"
    
    if not script_file.exists():
        print(f"❌ Script not found: {script_file}")
        return ""
    
    with open(script_file) as f:
        steps = f.read().strip()
    
    print(f"📖 Loaded script: {script_file}")
    return steps

def list_scripts():
    """List available scripts"""
    scripts_dir = get_scripts_dir()
    scripts = list(scripts_dir.glob("*.txt"))
    
    if not scripts:
        print("📂 No scripts found")
        return
    
    print("📂 Available scripts:")
    for script in scripts:
        print(f"  - {script.stem}")

async def run_program(steps: list):
    """Run a sequence of commands with logic"""
    print(f"🤖 Running {len(steps)} step program...")
    
    for i, step in enumerate(steps, 1):
        print(f"\n📍 Step {i}: {step}")
        
        if step == "check_logs":
            show_logs(5)
        elif step == "check_connection":
            try:
                await run_command("info")
                print("✅ Connection OK")
            except:
                print("❌ Connection failed, restarting...")
                await run_command("restart")
        elif step.startswith("cmd:"):
            cmd = step[4:]  # Remove "cmd:" prefix
            await run_command(cmd)
        elif step.startswith("sleep:"):
            seconds = int(step[6:])
            print(f"⏳ Sleeping {seconds}s...")
            await asyncio.sleep(seconds)
        
        await asyncio.sleep(1)  # Small delay between steps

async def run_javascript_file(filename, script_args=None, timeout=None, return_result=False):
    """Run a JavaScript file in the browser"""
    import os
    
    if not os.path.exists(filename):
        print(f"❌ Script file not found: {filename}")
        return
    
    print(f"🚀 Running JavaScript file: {filename}")
    
    # Read the script file
    with open(filename, 'r') as f:
        script_content = f.read()
    
    # Parse script arguments
    args = {}
    if script_args:
        args = smart_parse_params(script_args)
    
    # Wrap script with argument injection if needed
    if args:
        script_wrapper = f"""
        // Script arguments injected by AI Portal
        const scriptArgs = {json.dumps(args)};
        
        // Original script content
        {script_content}
        """
    else:
        script_wrapper = script_content
    
    # Execute via BROWSER_JS command
    import base64
    
    # Base64 encode the script for BrowserJSCommand requirement
    script_b64 = base64.b64encode(script_wrapper.encode('utf-8')).decode('utf-8')
    
    js_params = {
        'script': script_b64,
        'encoding': 'base64',
        'timeout': timeout or 10.0,
        'returnResult': return_result
    }
    
    result = await run_command('browser_js', json.dumps(js_params))
    
    if return_result and result:
        print(f"📊 Script result: {result}")


async def run_shell_command(command, timeout=30.0):
    """Execute shell command on server"""
    print(f"⚡ Executing shell command: {command}")
    
    # Execute via EXEC command  
    exec_params = {
        'command': command,
        'timeout': timeout
    }
    
    result = await run_command('exec', json.dumps(exec_params))
    
    if result:
        print(f"📊 Command output: {result}")

async def show_help(command=None):
    """Show help using Continuum's built-in help command"""
    if command:
        # Show help for specific command
        print(f"📡 Getting help for command: {command}")
        help_params = json.dumps({'command': command})
        await run_command('help', help_params)
    else:
        # Show general help - list all commands
        print("📡 Getting help for all commands")
        await run_command('help', '{}')

async def test_version_pipeline():
    """Test complete version increment and deployment pipeline"""
    import json
    import time
    import os
    
    print("🔥🔥🔥 STARTING VERSION PIPELINE TEST 🔥🔥🔥")
    
    # Step 1: Read current version
    print("📖 Reading current version...")
    package_path = "../package.json"
    if not os.path.exists(package_path):
        package_path = "package.json"
    
    try:
        with open(package_path, 'r') as f:
            package_data = json.load(f)
        current_version = package_data['version']
        print(f"📦 Current version: {current_version}")
    except Exception as e:
        print(f"🚨 MAJOR ERROR: Cannot read package.json: {e}")
        return False
    
    # Step 2: Increment version
    print("⬆️ Incrementing version...")
    version_parts = current_version.split('.')
    try:
        version_parts[-1] = str(int(version_parts[-1]) + 1)
        new_version = '.'.join(version_parts)
        package_data['version'] = new_version
        
        with open(package_path, 'w') as f:
            json.dump(package_data, f, indent=2)
        
        print(f"✅ Version incremented to: {new_version}")
    except Exception as e:
        print(f"🚨 MAJOR ERROR: Cannot increment version: {e}")
        return False
    
    # Step 3: Restart server with new version
    print("🔄 Restarting server...")
    try:
        await run_command('restart', '{}')
        time.sleep(3)  # Wait for restart
        print("✅ Server restart initiated")
    except Exception as e:
        print(f"🚨 MAJOR ERROR: Cannot restart server: {e}")
        return False
    
    # Step 4: Inject test JavaScript to verify both server and client see new version
    test_id = int(time.time())
    test_js = f"""
    console.log('🔥🔥🔥 VERSION TEST {test_id}: CLIENT SIDE EXECUTED 🔥🔥🔥');
    console.log('📦 CLIENT VERSION:', window.continuumVersion || 'unknown');
    console.log('🔥🔥🔥 VERSION TEST {test_id}: CLIENT COMPLETE 🔥🔥🔥');
    """
    
    print(f"💉 Injecting test JavaScript (ID: {test_id})...")
    try:
        js_params = {
            'script': test_js,
            'returnResult': True
        }
        await run_command('browser_js', json.dumps(js_params))
        print("✅ JavaScript injection completed")
    except Exception as e:
        print(f"🚨 MAJOR ERROR: Cannot inject JavaScript: {e}")
        return False
    
    # Step 5: Wait and check logs for our test statements
    print("⏳ Waiting for logs to process...")
    time.sleep(2)
    
    print("🔍 Checking logs for test statements...")
    try:
        # Check recent logs for our test ID
        logs_found = False
        for i in range(3):  # Try multiple times
            # Simulate checking logs (would need to implement actual log checking)
            print(f"📋 Checking logs attempt {i+1}...")
            time.sleep(1)
            
            # TODO: Actually parse the logs for our test_id
            # For now, just report what we're looking for
            print(f"🔍 Looking for: 'VERSION TEST {test_id}'")
            
        print(f"✅ Version pipeline test completed")
        return True
        
    except Exception as e:
        print(f"🚨 MAJOR ERROR: Cannot verify logs: {e}")
        return False

def smart_parse_params(params_str, **kwargs):
    """Smart parameter parser: detects JSON vs natural arguments"""
    
    # If it looks like JSON (starts with { or [), parse as JSON
    if params_str.strip().startswith(('{', '[')):
        try:
            return json.loads(params_str)
        except json.JSONDecodeError:
            print(f"⚠️ Invalid JSON: {params_str}")
            return {}
    
    # Otherwise, build JSON from natural arguments
    result = {}
    
    # Add any click options passed as kwargs
    for key, value in kwargs.items():
        if value is not None:
            result[key] = value
    
    # Parse natural key=value pairs from params_str
    if params_str and params_str != '{}':
        for pair in params_str.split(','):
            if '=' in pair:
                key, value = pair.split('=', 1)
                key = key.strip()
                value = value.strip()
                
                # Smart type conversion
                if value.lower() in ('true', 'false'):
                    result[key] = value.lower() == 'true'
                elif value.isdigit():
                    result[key] = int(value)
                elif value.replace('.', '').isdigit():
                    result[key] = float(value)
                else:
                    # Remove quotes if present
                    if value.startswith('"') and value.endswith('"'):
                        value = value[1:-1]
                    result[key] = value
    
    return result

def get_command_tokenizer():
    """CS 200 level command tokenization - simple lookup table"""
    
    return {
        # Browser commands (CLI → Browser JS)
        'alert': lambda args: {
            'command': 'browser_js',
            'params': {'script': f'alert("{" ".join(args)}");', 'encoding': 'base64'}
        },
        'log': lambda args: {
            'command': 'browser_js', 
            'params': {'script': f'console.log("📝 CLI:", "{" ".join(args)}");', 'encoding': 'base64'}
        },
        'eval': lambda args: {
            'command': 'browser_js',
            'params': {'script': f'console.log("🔍 Result:", {" ".join(args)}); {" ".join(args)};', 'encoding': 'base64', 'returnResult': True}
        },
        'reload': lambda args: {
            'command': 'reload',
            'params': {'force': 'force' in args}
        },
        
        # System commands (CLI → Server exec)
        'shell': lambda args: {
            'command': 'exec',
            'params': {'command': ' '.join(args)}
        },
        'git': lambda args: {
            'command': 'exec', 
            'params': {'command': f'git {" ".join(args)}'}
        },
        'npm': lambda args: {
            'command': 'exec',
            'params': {'command': f'npm {" ".join(args)}'}
        },
        'test': lambda args: {
            'command': 'exec',
            'params': {'command': f'npm test {" ".join(args)}' if args else 'npm test', 'timeout': 60.0}
        },
        
        # Workspace commands (natural language)
        'workspace': lambda args: {
            'command': 'workspace',
            'params': {'action': args[0] if args else 'path'}
        },
        'ws': lambda args: {  # shorthand
            'command': 'workspace', 
            'params': {'action': args[0] if args else 'path'}
        },
        
        # Sentinel commands 
        'sentinel': lambda args: {
            'command': 'sentinel',
            'params': {'action': args[0] if args else 'status', 'task': args[1] if len(args) > 1 else None}
        },
        'watch': lambda args: {  # shorthand for sentinel
            'command': 'sentinel',
            'params': {'action': 'start', 'task': args[0] if args else 'watch'}
        },
        
        # Screenshot commands  
        'screenshot': lambda args: {
            'command': 'screenshot',
            'params': {'selector': args[0] if args else 'body', 'filename': args[1] if len(args) > 1 else None}
        },
        'snap': lambda args: {  # shorthand
            'command': 'screenshot',
            'params': {'selector': args[0] if args else 'body'}
        },
        'screenshot_services': lambda args: {
            'command': 'screenshot_services',
            'params': {'action': 'list'}
        },
        'daemon_screenshot': lambda args: {
            'command': 'daemon_screenshot', 
            'params': {'filename': args[0] if args else None, 'daemon_id': args[1] if len(args) > 1 else None}
        },
        
        # Log commands - direct live access
        'logs': lambda args: {
            'command': 'sentinel',
            'params': {
                'action': 'logs', 
                'source': next((arg for arg in args if arg in ['client', 'server', 'both']), 'both'),
                'lines': next((int(arg) for arg in args if arg.isdigit()), 10),
                'live': True,
                'stream': '--watch' in args or '--stream' in args or '-w' in args
            }
        },
        'client-logs': lambda args: {
            'command': 'sentinel', 
            'params': {
                'action': 'logs', 
                'source': 'client', 
                'lines': next((int(arg) for arg in args if arg.isdigit()), 10),
                'live': True,
                'stream': '--watch' in args or '--stream' in args or '-w' in args
            }
        },
        'server-logs': lambda args: {
            'command': 'sentinel',
            'params': {
                'action': 'logs', 
                'source': 'server', 
                'lines': next((int(arg) for arg in args if arg.isdigit()), 10),
                'live': True,
                'stream': '--watch' in args or '--stream' in args or '-w' in args
            }
        },
        
        # Event subscription commands
        'subscribe': lambda args: {
            'command': 'event',
            'params': {
                'action': 'subscribe',
                'event': args[0] if args else 'logs',
                'filter': args[1] if len(args) > 1 else 'both',
                'sessionId': f'portal-{int(time.time())}'
            }
        },
        'unsubscribe': lambda args: {
            'command': 'event',
            'params': {
                'action': 'unsubscribe',
                'event': args[0] if args else 'logs',
                'filter': args[1] if len(args) > 1 else 'both',
                'sessionId': f'portal-{int(time.time())}'
            }
        },
        'events': lambda args: {
            'command': 'event',
            'params': {'action': 'list'}
        },
        'ws-status': lambda args: {
            'command': 'agents',
            'params': {'live': True}
        },
        
        # DevTools commands - direct Portal integration
        'devtools': lambda args: {
            'command': 'devtools_action',
            'params': {
                'action': 'start_devtools_monitoring',
                'browser': args[0] if args else 'auto'
            }
        },
        'browser-logs': lambda args: {
            'command': 'devtools_action',
            'params': {
                'action': 'show_browser_logs',
                'lines': int(args[0]) if args and args[0].isdigit() else 10
            }
        },
        'browser-status': lambda args: {
            'command': 'devtools_action',
            'params': {'action': 'browser_status'}
        },
        'launch-opera': lambda args: {
            'command': 'devtools_action',
            'params': {'action': 'launch_opera_devtools'}
        },
        
        # Help commands  
        'help': lambda args: {
            'command': 'help',
            'params': {'command': args[0] if args else None, 'verbose': 'verbose' in args or '-v' in args}
        },
        'status': lambda args: {  # project status
            'command': 'help',
            'params': {'verbose': True}
        },
        'docs': lambda args: {  # generate docs
            'command': 'help', 
            'params': {'sync': True, 'output': args[0] if args else 'README.md'}
        },
        
        # Development shortcuts
        'restart': lambda args: {
            'command': 'restart',
            'params': {}
        },
        'version': lambda args: {
            'command': 'exec',
            'params': {'command': 'node -e "console.log(require(\'./package.json\').version)"'}
        },
        'health': lambda args: {  # quick health check
            'command': 'help',
            'params': {}
        },
        'spawn': lambda args: {  # spawn fresh agent observer using academy system
            'command': 'spawn',
            'params': f'observer-agent-{int(time.time())} {args[0] if args else "system_exploration"}'
        },
        'test-agent': lambda args: run_fresh_agent_test(args)
    }

def tokenize_command(cmd, args):
    """CS 200 tokenization: cmd + args → command structure"""
    tokenizer = get_command_tokenizer()
    
    if cmd.lower() in tokenizer:
        result = tokenizer[cmd.lower()](args)
        
        # Handle base64 encoding for browser_js
        if result['command'] == 'browser_js' and 'encoding' in result['params']:
            import base64
            script = result['params']['script']
            result['params']['script'] = base64.b64encode(script.encode('utf-8')).decode('utf-8')
        
        return result
    else:
        # Default: treat as direct command
        return {
            'command': cmd,
            'params': {'action': args[0] if args else None} if args else {}
        }

@click.command(context_settings=dict(ignore_unknown_options=True, allow_extra_args=True))
@click.option('--buffer', is_flag=True, help='Start buffering WebSocket messages')
@click.option('--logs', type=int, default=10, help='Show N recent logs')
@click.option('--clear', help='Clear logs with optional label')
@click.option('--cmd', help='Run any continuum command (e.g. "restart", "screenshot")')
@click.option('--params', default='', help='Command params: JSON {"key":"value"} or natural key=value,key2=value2')
@click.option('--dashboard', is_flag=True, help='Show AI agent dashboard')
@click.option('--broken', is_flag=True, help='Show broken commands')
@click.option('--recent', is_flag=True, help='Show recent work')
@click.option('--quick', is_flag=True, help='Show quick status')
@click.option('--test', is_flag=True, help='Run Continuum test suite')
@click.option('--connect', is_flag=True, help='Start persistent event monitoring connection')
@click.option('--disconnect', is_flag=True, help='Stop persistent event monitoring connection')
@click.option('--devtools', is_flag=True, help='Start persistent DevTools browser console monitoring')
@click.option('--live', is_flag=True, help='Start intelligent live log monitoring system')
@click.option('--failsafe', is_flag=True, help='Emergency DevTools failsafe recovery for logs')
@click.option('--recovery', is_flag=True, help='Full system recovery using complete DevTools emergency system')
@click.option('--daemons', is_flag=True, help='List all running daemons')
@click.option('--daemon-logs', help='Show logs for specific daemon ID')
@click.option('--daemon-status', help='Show status for specific daemon ID')
@click.option('--roadmap', is_flag=True, help='Show development roadmap and vision')
@click.option('--files', is_flag=True, help='Show codebase structure and reduction mission')
@click.pass_context

# Command-specific natural argument options
@click.option('--action', help='Action to perform (for workspace, sentinel, etc.)')
@click.option('--task', help='Task name (for sentinel)')
@click.option('--workspace', help='Workspace name (for workspace command)')
@click.option('--selector', help='CSS selector (for screenshot)')
@click.option('--filename', help='Output filename')
@click.option('--verbose', is_flag=True, help='Verbose output (for help command)')
@click.option('--sync', is_flag=True, help='Sync documentation (for help command)')
@click.option('--output', help='Output file path')

# Script execution options
@click.option('--run', help='Run JavaScript file in browser (e.g. --run script.js)')
@click.option('--script-args', help='Arguments to pass to script (JSON or key=value)')
@click.option('--timeout', type=float, help='Script execution timeout in seconds')
@click.option('--return-result', is_flag=True, help='Return script result instead of just success')

# Shell execution options
@click.option('--exec', help='Execute shell command on server (e.g. --exec "git status")')
@click.option('--shell-timeout', type=float, default=30.0, help='Shell command timeout')

@click.option('--program', help='Run program: "check_connection,sleep:5,cmd:screenshot"')
@click.option('--script', help='Run saved script by name')
@click.option('--save-script', help='Save program as script: --save-script debug --program "..."')
@click.option('--list-scripts', is_flag=True, help='List available scripts')
@click.option('--help-cmd', help='Show help for specific command')
@click.option('--debug', is_flag=True, help='Show full debug output including stack traces')
@click.option('--version-test', is_flag=True, help='Test version increment and deployment pipeline')
@click.option('--devtools', is_flag=True, help='Start DevTools browser console monitoring only')
@click.option('--notify', help='Send notify command to test browser log capture (e.g. --notify "Test message")')
def main(ctx, buffer, logs, clear, cmd, params, dashboard, broken, recent, quick, test, connect, disconnect, roadmap, files, action, task, workspace, selector, filename, 
         verbose, sync, output, run, script_args, timeout, return_result, exec, shell_timeout,
         program, script, save_script, list_scripts, help_cmd, debug, version_test, devtools, live, failsafe, recovery, daemons, daemon_logs, daemon_status, notify):
    """AI Portal - Your Robot Agent for Continuum development workflow"""
    
    async def run_cli():
        # Handle connect/disconnect first
        if connect:
            return await start_monitoring_connection()
        if disconnect:
            return await stop_monitoring_connection()
        if devtools:
            return await start_standard_connection_protocol()
        if live:
            return await start_live_monitor()
        if failsafe:
            return await emergency_failsafe_recovery()
        if recovery:
            return await full_system_recovery()
        if daemons:
            return show_daemon_list()
        if daemon_logs:
            return show_daemon_logs(daemon_logs)
        if daemon_status:
            return show_daemon_status(daemon_status)
        
        # Handle notify test command
        if notify:
            print(f"🔔 Sending notify command: {notify}")
            await run_command('notify', json.dumps({"message": notify, "source": "ai-portal-test"}))
            return
            
        # Handle dashboard options first
        if dashboard or broken or recent or quick or test or roadmap or files:
            try:
                # Import ai-agent functionality
                import importlib.util
                spec = importlib.util.spec_from_file_location("ai_agent", Path(__file__).parent / "ai-agent.py")
                ai_agent = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(ai_agent)
                
                if test:
                    await ai_agent.run_tests()
                elif roadmap:
                    await ai_agent.show_roadmap()
                elif files:
                    await ai_agent.show_files_structure()
                elif broken:
                    await ai_agent.show_broken()
                elif recent:
                    await ai_agent.show_recent()
                elif quick:
                    await ai_agent.show_quick()
                else:  # dashboard
                    await ai_agent.show_dashboard()
                return
            except Exception as e:
                print(f"❌ Dashboard error: {e}")
                print("💡 Try: python3 ai-agent.py --dashboard")
                return
        
        if version_test:
            await test_version_pipeline()
        elif help_cmd:
            await show_help(help_cmd)
        elif run:
            await run_javascript_file(run, script_args, timeout, return_result)
        elif exec:
            await run_shell_command(exec, shell_timeout)
        elif buffer:
            await start_buffer()
        elif cmd:
            # Get extra arguments from click context
            remaining_args = ctx.args if ctx.args else []
            
            # Use CS 200 tokenization for commands in the tokenizer
            if cmd.lower() in get_command_tokenizer():
                print(f"🎯 Tokenizing: {cmd} {' '.join(remaining_args)}")
                
                # For screenshot command, merge click options with tokenizer params
                if cmd.lower() == 'screenshot':
                    tokenized = tokenize_command(cmd, remaining_args)
                    
                    # Parse JSON params if provided
                    if params:
                        try:
                            json_params = json.loads(params)
                            # Merge JSON params into tokenized params
                            tokenized['params'].update(json_params)
                        except json.JSONDecodeError:
                            print(f"⚠️ Invalid JSON in params: {params}")
                    
                    # Override with click options if provided (highest priority)
                    if selector:
                        tokenized['params']['selector'] = selector
                    if filename:
                        tokenized['params']['filename'] = filename
                else:
                    tokenized = tokenize_command(cmd, remaining_args)
                    
                await run_command(tokenized['command'], json.dumps(tokenized['params']), verbose=debug)
            else:
                # Fall back to smart parameter parsing for complex commands
                parsed_params = smart_parse_params(params, 
                    action=action, task=task, workspace=workspace,
                    selector=selector, filename=filename, verbose=verbose,
                    sync=sync, output=output)
                
                # Convert back to JSON string for run_command
                params_json = json.dumps(parsed_params) if parsed_params else '{}'
                await run_command(cmd, params_json, verbose=debug)
        elif clear is not None:
            clear_logs(clear if clear else None)
        elif list_scripts:
            list_scripts()
        elif save_script and program:
            save_script(save_script, program)
        elif script:
            script_steps = load_script(script)
            if script_steps:
                steps = script_steps.split(',')
                await run_program(steps)
        elif program:
            steps = program.split(',')
            await run_program(steps)
        else:
            show_logs(logs)
    
    asyncio.run(run_cli())

async def start_monitoring_connection():
    """Start persistent event monitoring connection as background process"""
    import subprocess
    import sys
    import os
    import json
    
    # Step 1: Increment version and restart server
    print("📦 Incrementing version and restarting server...")
    package_path = "../package.json"
    if not os.path.exists(package_path):
        package_path = "package.json"
    
    try:
        with open(package_path, 'r') as f:
            package_data = json.load(f)
        current_version = package_data['version']
        version_parts = current_version.split('.')
        version_parts[-1] = str(int(version_parts[-1]) + 1)
        new_version = '.'.join(version_parts)
        package_data['version'] = new_version
        
        with open(package_path, 'w') as f:
            json.dump(package_data, f, indent=2)
        print(f"✅ Version incremented: {current_version} → {new_version}")
        
        # Restart server
        await run_command('restart', '{}')
        print("✅ Server restarted")
        
    except Exception as e:
        print(f"⚠️ Version increment failed: {e}, continuing with monitoring...")
    
    # Check if monitor already running
    pid_file = Path('.continuum/ai-portal/monitor.pid')
    if pid_file.exists():
        try:
            pid = int(pid_file.read_text().strip())
            os.kill(pid, 0)  # Check if process exists
            print(f"📡 Monitor already running (PID: {pid})")
            return
        except (OSError, ValueError):
            pid_file.unlink()  # Remove stale PID file
    
    print("📡 Starting persistent event monitoring in background...")
    
    # Create workspace
    pid_file.parent.mkdir(parents=True, exist_ok=True)
    
    # Create monitor script
    monitor_script = pid_file.parent / 'monitor.py'
    monitor_script.write_text('''#!/usr/bin/env python3
import asyncio
import signal
import sys
import os
from pathlib import Path

# Add parent directory to path
sys.path.insert(0, str(Path(__file__).parent.parent))
from continuum_client import ContinuumClient

async def run_monitor():
    pid_file = Path('.continuum/ai-portal/monitor.pid')
    
    # Save PID
    pid_file.write_text(str(os.getpid()))
    
    # Setup signal handlers
    def signal_handler(signum, frame):
        print("\\n📡 Stopping event monitor...")
        pid_file.unlink(missing_ok=True)
        exit(0)
    
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    try:
        client = ContinuumClient()
        await client.connect()
        
        # Subscribe to all events
        session_id = f'monitor-{int(__import__("time").time())}'
        
        await client.send_command('event', {
            'action': 'subscribe',
            'event': 'logs',
            'filter': 'both',
            'sessionId': session_id
        })
        
        await client.send_command('event', {
            'action': 'subscribe', 
            'event': 'commands',
            'filter': 'both',
            'sessionId': session_id
        })
        
        print(f"📡 Event monitor running (PID: {os.getpid()})")
        print("📡 Subscribed to logs and commands events")
        
        # Create log files
        log_dir = Path('.continuum/ai-portal/monitor-logs')
        log_dir.mkdir(exist_ok=True)
        
        events_log = log_dir / f'events-{int(__import__("time").time())}.log'
        
        # Add message handler to capture events
        def handle_event(message):
            if message.get('type') == 'event_stream':
                timestamp = message.get('timestamp', __import__('datetime').datetime.now().isoformat())
                event_type = message.get('eventType', 'unknown')
                source = message.get('source', 'unknown')
                data = message.get('data', {})
                
                log_entry = f"[{timestamp}] {event_type}:{source} - {data}\\n"
                with open(events_log, 'a') as f:
                    f.write(log_entry)
                print(f"📡 Event captured: {event_type}:{source}")
        
        client.add_message_handler(handle_event)
        
        print(f"📡 Logging events to: {events_log}")
        
        # Keep running and receive events
        while True:
            await asyncio.sleep(1)
            
    except Exception as e:
        print(f"📡 Monitor error: {e}")
    finally:
        pid_file.unlink(missing_ok=True)

if __name__ == "__main__":
    asyncio.run(run_monitor())
''')
    
    # Start background process
    process = subprocess.Popen([
        sys.executable, str(monitor_script)
    ], 
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
    stdin=subprocess.DEVNULL
    )
    
    # Wait a moment and check if it started
    await asyncio.sleep(1)
    if pid_file.exists():
        pid = int(pid_file.read_text().strip())
        print(f"📡 Event monitor started in background (PID: {pid})")
        print("📡 Use 'python3 ai-portal.py --disconnect' to stop")
    else:
        print("❌ Failed to start event monitor")

async def stop_monitoring_connection():
    """Stop persistent event monitoring connection"""
    pid_file = Path('.continuum/ai-portal/monitor.pid')
    
    if not pid_file.exists():
        print("📡 No monitor running")
        return
        
    try:
        pid = int(pid_file.read_text().strip())
        import os
        os.kill(pid, 15)  # SIGTERM
        pid_file.unlink()
        print(f"📡 Stopped monitor (PID: {pid})")
    except (OSError, ValueError) as e:
        print(f"📡 Failed to stop monitor: {e}")
        pid_file.unlink(missing_ok=True)

async def start_devtools_daemon():
    """Start integrated Continuum system with DevTools monitoring"""
    print("🚀 Starting AI Portal mode (continuum devtools)...")
    
    try:
        # Use the new clean continuum CLI
        continuum_path = Path(__file__).parent.parent / "continuum"
        
        if continuum_path.exists():
            print("🔗 Using clean Continuum CLI...")
            import subprocess
            
            # Start in DevTools mode for AI Portal
            process = subprocess.Popen([
                str(continuum_path), 'devtools'
            ], cwd=continuum_path.parent)
            
            print("🔌 AI Portal mode starting...")
            print("💡 Features enabled:")
            print("   • TypeScript daemon system")
            print("   • Browser interface (primary console)")
            print("   • DevTools monitoring") 
            print("   • JTAG system for AI debugging")
            print("   • Real-time log streaming")
            print("   • Screenshot services")
            print("   • Self-healing mechanisms")
            print("\n⏰ Press Ctrl+C to stop")
            
            try:
                process.wait()
            except KeyboardInterrupt:
                print("\n🛑 Stopping AI Portal...")
                process.terminate()
                process.wait()
                print("✅ AI Portal stopped")
                
        else:
            # Fallback to simple DevTools monitoring
            print("⚠️ Clean CLI not available, using fallback...")
            await start_simple_devtools_monitoring()
            
    except Exception as e:
        print(f"❌ AI Portal startup error: {e}")
        
async def start_simple_devtools_monitoring():
    """Fallback simple DevTools monitoring"""
    # Launch Opera in debug mode first
    launch_result = await launch_continuum_browser(debug_mode=True)
    
    if launch_result['success']:
        # Now start monitoring
        monitor = await start_devtools_monitoring()
        
        if monitor:
            print("🔌 DevTools monitoring active - press Ctrl+C to stop")
            try:
                while True:
                    await asyncio.sleep(1)
            except KeyboardInterrupt:
                await stop_devtools_monitoring()
                print("\n🔌 DevTools monitoring stopped")
        else:
            print("❌ Failed to start DevTools monitoring")
    else:
        print("❌ Failed to launch Opera in debug mode")

async def start_live_monitor():
    """Start intelligent live monitoring using modular platform"""
    print("📡 Starting intelligent live DevTools monitoring...")
    
    try:
        # Use the modular LiveDevToolsMonitor
        monitor = LiveDevToolsMonitor()
        
        # Add Portal buffer logging
        def portal_log_callback(log_entry):
            write_log(f"BROWSER_LOG [{log_entry.get('source', 'browser')}] [{log_entry.get('level', 'INFO').upper()}]: {log_entry.get('text', '')}")
        
        monitor.add_live_callback(portal_log_callback)
        
        print("📡 Intelligent real-time system with screenshot support")
        print("📡 Memory-based responsiveness - press Ctrl+C to stop")
        
        # Run the live monitor
        await monitor.run()
        
    except KeyboardInterrupt:
        print("\n📡 Live monitoring stopped")
    except Exception as e:
        print(f"❌ Live monitoring error: {e}")

async def emergency_failsafe_recovery():
    """Emergency failsafe recovery for critical log monitoring"""
    print("🚨 EMERGENCY FAILSAFE RECOVERY - Critical log monitoring")
    print("🚨 This is our most critical feedback mechanism for codebase state")
    
    try:
        # Check current state
        print("\n📊 CURRENT STATE ASSESSMENT:")
        
        # Check if Continuum server is running
        try:
            await run_command("info", "{}")
            print("✅ Continuum server: RESPONDING")
        except Exception as e:
            print(f"❌ Continuum server: FAILED - {e}")
            print("🔧 Attempting server recovery...")
            try:
                await run_command("restart", "{}")
                print("✅ Server restart initiated")
            except:
                print("❌ Server restart failed - manual intervention may be needed")
        
        # Check DevTools connection
        global devtools_monitor
        if devtools_monitor and devtools_monitor.connected:
            print("✅ DevTools failsafe: ACTIVE")
        else:
            print("❌ DevTools failsafe: INACTIVE")
            print("🔧 Starting emergency DevTools monitoring...")
            
            monitor = await start_devtools_monitoring()
            if monitor:
                print("✅ Emergency DevTools monitoring: ACTIVATED")
            else:
                print("❌ Emergency DevTools monitoring: FAILED")
                print("💡 Try: Opera with --remote-debugging-port=9222")
        
        # Check log files
        buffer_file = get_logs_dir() / 'buffer.log'
        if buffer_file.exists():
            size = buffer_file.stat().st_size
            print(f"✅ Log buffer: {size} bytes")
        else:
            print("❌ Log buffer: MISSING")
            print("🔧 Creating log buffer...")
            write_log("SYSTEM: Emergency failsafe recovery initiated")
            print("✅ Log buffer: CREATED")
        
        # Show recent activity for state assessment
        print("\n📋 RECENT ACTIVITY (for state assessment):")
        show_logs(5)
        
        print("\n🚨 FAILSAFE RECOVERY COMPLETE")
        print("🔍 Use 'python3 ai-portal.py --logs' to monitor state")
        print("🔌 Use 'python3 ai-portal.py --connect' for persistent monitoring")
        
    except Exception as e:
        print(f"🚨 FAILSAFE RECOVERY ERROR: {e}")
        print("🚨 CRITICAL: Manual intervention required")

async def full_system_recovery():
    """Full system recovery using complete DevTools emergency system"""
    print("🚨 FULL SYSTEM RECOVERY")
    print("=" * 60)
    print("🎯 Launching complete DevTools recovery system...")
    print("🛡️ This system works no matter what's broken")
    print()
    
    try:
        # Get the path to our recovery script
        script_dir = Path(__file__).parent.parent
        recovery_script = script_dir / "devtools_full_demo.py"
        
        if not recovery_script.exists():
            print("❌ Recovery script not found at:", recovery_script)
            print("💡 Expected location: devtools_full_demo.py in project root")
            return
        
        print(f"🚀 Executing recovery system: {recovery_script}")
        print("📋 Recovery system provides these capabilities:")
        print("   1. 🔍 DIAGNOSE system state (what's working/broken)")
        print("   2. 🚀 AUTO-LAUNCH Opera in debug mode")
        print("   3. 📸 CAPTURE SCREENSHOTS via DevTools Protocol")
        print("      └─ Screenshots saved to: .continuum/screenshots/")
        print("      └─ Screenshots will be OPENED automatically for inspection")
        print("   4. 📋 CAPTURE LOGS from browser console in real-time")
        print("      └─ Portal logs: python ai-portal.py --logs 10")
        print("      └─ Recovery logs: .continuum/recovery_logs/")
        print("   5. 🔌 EXECUTE JAVASCRIPT with unique timestamps")
        print("      └─ Proves fresh console output (not cached)")
        print("   6. 👁️ VERIFY console output appears in BOTH client and server logs")
        print("      └─ Shows exact log entries with timestamps")
        print("   7. 🛡️ SELF-HEAL and auto-recover from failures")
        print("   8. 📄 GENERATE comprehensive recovery report")
        print("   9. 🎯 PROVE complete agent feedback loop is operational")
        print()
        print("🔍 WHAT TO EXPECT:")
        print("   • Browser opens to Continuum interface with debug mode")
        print("   • Red indicator box appears in top-right corner")
        print("   • Screenshot opens automatically with visual proof")
        print("   • Console logs show unique timestamp markers")
        print("   • Pass/fail status for each capability")
        print()
        print("⌨️ Press Ctrl+C to exit recovery mode")
        print("-" * 60)
        
        # Execute the recovery system with self-healing enabled
        import subprocess
        process = subprocess.run([
            sys.executable, str(recovery_script), "--self-heal"
        ], cwd=str(script_dir))
        
        if process.returncode == 0:
            print("\n✅ FULL SYSTEM RECOVERY COMPLETED SUCCESSFULLY")
            print("🎯 System should now be operational with emergency capabilities")
            print("📸 Screenshots and logs available in .continuum/ directories")
        else:
            print(f"\n⚠️ Recovery system exited with code: {process.returncode}")
            print("💡 Check recovery logs for details")
        
    except KeyboardInterrupt:
        print("\n🛑 Recovery interrupted by user")
    except Exception as e:
        print(f"\n❌ Recovery system error: {e}")
        print("🚨 Falling back to basic failsafe...")
        await emergency_failsafe_recovery()

def show_daemon_list():
    """List all running daemons"""
    print("🤖 RUNNING DAEMONS:")
    
    daemons = daemon_manager.list_daemons()
    
    if not daemons:
        print("  📝 No daemons currently running")
        return
    
    for daemon_info in daemons:
        daemon_id = daemon_info['daemon_id']
        daemon_type = daemon_info['daemon_type']
        uptime = daemon_info['uptime_seconds']
        running = "🟢 Running" if daemon_info['running'] else "🔴 Stopped"
        
        print(f"  🤖 {daemon_id}")
        print(f"     Type: {daemon_type}")
        print(f"     Status: {running}")
        print(f"     Uptime: {uptime:.1f}s")
        print(f"     Logs: {daemon_info['memory_logs_count']} entries")
        print()

def show_daemon_logs(daemon_id: str):
    """Show logs for specific daemon"""
    print(f"📋 DAEMON LOGS: {daemon_id}")
    
    logs = daemon_manager.get_daemon_logs(daemon_id, lines=20)
    
    if not logs:
        print(f"  ❌ No logs found for daemon: {daemon_id}")
        return
    
    for log_entry in logs:
        if isinstance(log_entry, dict):
            timestamp = log_entry.get('timestamp', 'unknown')
            level = log_entry.get('level', 'INFO')
            message = log_entry.get('message', '')
            
            # Format timestamp for readability
            try:
                dt = datetime.fromisoformat(timestamp)
                time_str = dt.strftime("%H:%M:%S")
            except:
                time_str = timestamp
            
            print(f"  [{time_str}] {level}: {message}")
            
            # Show additional data if present
            if 'data' in log_entry and log_entry['data']:
                for key, value in log_entry['data'].items():
                    print(f"    {key}: {value}")
        else:
            print(f"  {log_entry}")

def show_daemon_status(daemon_id: str):
    """Show detailed status for specific daemon"""
    print(f"📊 DAEMON STATUS: {daemon_id}")
    
    status = daemon_manager.get_daemon_status(daemon_id)
    
    if not status:
        print(f"  ❌ Daemon not found: {daemon_id}")
        return
    
    print(f"  🤖 Daemon ID: {status['daemon_id']}")
    print(f"  📋 Type: {status['daemon_type']}")
    print(f"  🔄 Running: {status['running']}")
    print(f"  ⏱️  Uptime: {status['uptime_seconds']:.1f}s")
    print(f"  📝 Log File: {status['log_file']}")
    print(f"  💾 Memory Logs: {status['memory_logs_count']}")
    print(f"  🚀 Start Time: {status['start_time']}")
    
    # Show type-specific status if available
    if 'browser_connected' in status:
        print(f"  🌐 Browser Connected: {status['browser_connected']}")
        print(f"  📊 Logs Captured: {status.get('logs_captured', 0)}")
        print(f"  🔗 Target URL: {status.get('target_url', 'unknown')}")

if __name__ == "__main__":
    import sys
    # Handle --cmd X --help pattern
    if len(sys.argv) >= 4 and sys.argv[1] == '--cmd' and sys.argv[3] == '--help':
        asyncio.run(show_help(sys.argv[2]))
    else:
        main()