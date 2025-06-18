#!/usr/bin/env python3
"""
AI Portal - Continuum Command Interface
=======================================
THE primary thin client for Continuum's command bus architecture.

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
"""

import asyncio
import json
import click
from pathlib import Path
from datetime import datetime

import sys
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

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
    logs_dir = get_logs_dir()
    buffer_file = logs_dir / 'buffer.log'
    
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    with open(buffer_file, 'a') as f:
        f.write(f"[{timestamp}] {message}\n")

async def handle_message(message):
    """Simple message logger"""
    msg_type = message.get('type', 'unknown')
    write_log(f"{msg_type}: {json.dumps(message)}")

async def start_buffer():
    """Start buffering WebSocket messages"""
    print("🚀 Starting buffer...")
    
    try:
        load_continuum_config()
        
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'ai-portal',
                'agentName': 'AI Portal',
                'agentType': 'ai'
            })
            
            client.add_message_handler(handle_message)
            write_log("SYSTEM: Buffer started")
            
            print(f"📁 Logging to: {get_logs_dir()}/buffer.log")
            print("🔄 Buffering (Ctrl+C to stop)")
            
            while True:
                await asyncio.sleep(1)
                
    except KeyboardInterrupt:
        write_log("SYSTEM: Buffer stopped")
        print("\n👋 Buffer stopped")

def show_logs(lines: int = 10):
    """Show recent log lines"""
    buffer_file = get_logs_dir() / 'buffer.log'
    
    if not buffer_file.exists():
        print("📝 No logs found")
        return
        
    with open(buffer_file) as f:
        all_lines = f.readlines()
        recent = all_lines[-lines:] if len(all_lines) >= lines else all_lines
        
    print(f"📋 Last {len(recent)} log entries:")
    for line in recent:
        print(f"  {line.strip()}")

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

async def run_command(cmd: str, params: str = "{}"):
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
                    # Continuum command via send_command
                    import json
                    params_dict = json.loads(params) if params != "{}" else {}
                    result = await client.send_command(cmd, params_dict)
                
                print(f"✅ Result: {result}")
                write_log(f"COMMAND: {cmd} {params} -> {result}")
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

@click.command()
@click.option('--buffer', is_flag=True, help='Start buffering WebSocket messages')
@click.option('--logs', type=int, default=10, help='Show N recent logs')
@click.option('--clear', help='Clear logs with optional label')
@click.option('--cmd', help='Run any continuum command (e.g. "restart", "screenshot")')
@click.option('--params', default='{}', help='JSON params for command (default: {})')
@click.option('--program', help='Run program: "check_connection,sleep:5,cmd:screenshot"')
@click.option('--script', help='Run saved script by name')
@click.option('--save-script', help='Save program as script: --save-script debug --program "..."')
@click.option('--list-scripts', is_flag=True, help='List available scripts')
@click.option('--help-cmd', help='Show help for specific command')
def main(buffer, logs, clear, cmd, params, program, script, save_script, list_scripts, help_cmd):
    """AI Portal - Your Robot Agent for Continuum development workflow"""
    
    async def run():
        if help_cmd:
            await show_help(help_cmd)
        elif buffer:
            await start_buffer()
        elif cmd:
            await run_command(cmd, params)
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
    
    asyncio.run(run())

if __name__ == "__main__":
    import sys
    # Handle --cmd X --help pattern
    if len(sys.argv) >= 4 and sys.argv[1] == '--cmd' and sys.argv[3] == '--help':
        asyncio.run(show_help(sys.argv[2]))
    else:
        main()