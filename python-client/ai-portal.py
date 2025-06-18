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
                    # Continuum command via send_command
                    import json
                    params_dict = json.loads(params) if params != "{}" else {}
                    result = await client.send_command(cmd, params_dict)
                
                # Clean output by default, verbose if requested
                if verbose:
                    print(f"✅ Result: {result}")
                else:
                    if result.get('result', {}).get('success', True):
                        print(f"✅ {result.get('result', {}).get('message', 'Command completed')}")
                    else:
                        error_msg = result.get('result', {}).get('error', 'Unknown error')
                        print(f"❌ {result.get('result', {}).get('message', 'Command failed')}")
                        if isinstance(error_msg, str) and error_msg != 'Unknown error':
                            print(f"   {error_msg}")
                
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
    js_params = {
        'script': script_wrapper,
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
            'command': 'browser_js',
            'params': {'script': 'location.reload();', 'encoding': 'base64'}
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
            'params': {'command': f'npm test {" ".join(args)}' if args else 'npm test'}
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
def main(ctx, buffer, logs, clear, cmd, params, dashboard, broken, recent, quick, test, roadmap, files, action, task, workspace, selector, filename, 
         verbose, sync, output, run, script_args, timeout, return_result, exec, shell_timeout,
         program, script, save_script, list_scripts, help_cmd, debug):
    """AI Portal - Your Robot Agent for Continuum development workflow"""
    
    async def run_cli():
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
        
        if help_cmd:
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

if __name__ == "__main__":
    import sys
    # Handle --cmd X --help pattern
    if len(sys.argv) >= 4 and sys.argv[1] == '--cmd' and sys.argv[3] == '--help':
        asyncio.run(show_help(sys.argv[2]))
    else:
        main()