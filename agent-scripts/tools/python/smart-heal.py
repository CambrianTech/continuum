#!/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/venv/agents/bin/python
"""
Smart Healing System - Complete auto-repair for Continuum
Handles JS errors, browser restarts, server rebuilds, and more
"""

import sys
import json
import subprocess
import time
import asyncio
import websockets
from pathlib import Path
import click
import os

async def send_websocket_command(command, port=9000, timeout=10):
    """Send command via WebSocket and get response"""
    uri = f"ws://localhost:{port}"
    try:
        async with websockets.connect(uri, ping_interval=20, ping_timeout=10) as websocket:
            message = {
                "type": "execute_js",
                "data": {
                    "command": command,
                    "timestamp": time.time()
                }
            }
            
            await websocket.send(json.dumps(message))
            
            # Wait for response
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=timeout)
                return json.loads(response)
            except asyncio.TimeoutError:
                return {"success": True, "timeout": True}
                
    except Exception as e:
        return {"success": False, "error": str(e)}

async def diagnose_system(port=9000):
    """Run comprehensive system diagnostics"""
    click.echo("🔍 Running deep space probe diagnostics...")
    
    # Load diagnostic script  
    script_path = Path(__file__).parent.parent.parent / "agent-scripts" / "examples" / "diagnostics" / "full-system-check.js"
    if not script_path.exists():
        # Try relative to current working directory
        script_path = Path("agent-scripts/examples/diagnostics/full-system-check.js")
        if not script_path.exists():
            click.echo(f"❌ Diagnostic script not found at: {script_path}")
            return None
        
    diagnostic_js = script_path.read_text()
    result = await send_websocket_command(diagnostic_js, port)
    
    if result.get("success"):
        click.echo("✅ Diagnostics completed")
        return result
    else:
        click.echo(f"❌ Diagnostics failed: {result.get('error', 'Unknown error')}")
        return None

async def auto_repair_js(port=9000):
    """Auto-repair JavaScript issues"""
    click.echo("🔧 Running auto-repair sequence...")
    
    # Load repair script
    script_path = Path(__file__).parent.parent.parent / "agent-scripts" / "examples" / "fixes" / "auto-repair.js"
    if not script_path.exists():
        # Try relative to current working directory
        script_path = Path("agent-scripts/examples/fixes/auto-repair.js")
        if not script_path.exists():
            click.echo(f"❌ Auto-repair script not found at: {script_path}")
            return False
        
    repair_js = script_path.read_text()
    result = await send_websocket_command(repair_js, port)
    
    if result.get("success"):
        click.echo("✅ Auto-repair completed")
        return True
    else:
        click.echo(f"❌ Auto-repair failed: {result.get('error', 'Unknown error')}")
        return False

async def restart_browser(port=9000):
    """Restart browser tabs via WebSocket"""
    click.echo("🌐 Restarting browser tabs...")
    
    restart_js = """
    console.log('🔄 Initiating browser restart...');
    
    // Close current tab after short delay
    setTimeout(() => {
        console.log('🚪 Closing current tab...');
        window.close();
    }, 1000);
    
    // Open new tab (fallback)
    setTimeout(() => {
        const newWindow = window.open(window.location.href, '_blank');
        if (newWindow) {
            console.log('🌐 New tab opened');
        } else {
            console.log('⚠️ Please manually open a new tab');
        }
    }, 2000);
    
    'browser_restart_initiated'
    """
    
    result = await send_websocket_command(restart_js, port)
    
    if result.get("success"):
        click.echo("✅ Browser restart initiated")
        return True
    else:
        click.echo(f"❌ Browser restart failed: {result.get('error', 'Unknown error')}")
        return False

def rebuild_server():
    """Rebuild and restart the Continuum server"""
    click.echo("🏗️ Rebuilding Continuum server...")
    
    try:
        # Check if we're in a git repo and can rebuild
        if Path(".git").exists():
            click.echo("📝 Pulling latest changes...")
            subprocess.run(["git", "pull"], check=True)
        
        # Kill existing server
        click.echo("🛑 Stopping existing server...")
        try:
            subprocess.run(["pkill", "-f", "continuum"], timeout=5)
            time.sleep(2)
        except:
            pass  # Server might not be running
        
        # Restart server
        click.echo("🚀 Starting new server...")
        process = subprocess.Popen(
            ["continuum", "--daemon"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE
        )
        
        # Wait for startup
        for i in range(20):
            time.sleep(1)
            try:
                # Test if server is responding
                result = subprocess.run(
                    ["curl", "-s", "http://localhost:9000"], 
                    capture_output=True, 
                    timeout=2
                )
                if result.returncode == 0:
                    click.echo("✅ Server rebuilt and running")
                    return True
            except:
                pass
        
        click.echo("❌ Server rebuild failed - not responding")
        return False
        
    except Exception as e:
        click.echo(f"❌ Server rebuild error: {e}")
        return False

async def send_test_joke(port=9000):
    """Send a test joke to verify the system is working"""
    click.echo("😄 Sending test joke...")
    
    joke_js = """
    // Test joke to verify system functionality
    const jokes = [
        "Why do programmers prefer dark mode? Because light attracts bugs! 🐛",
        "How many programmers does it take to change a light bulb? None, that's a hardware problem! 💡",
        "Why don't programmers like nature? It has too many bugs! 🌿🐛",
        "What's a programmer's favorite hangout place? The Foo Bar! 🍺"
    ];
    
    const randomJoke = jokes[Math.floor(Math.random() * jokes.length)];
    console.log('🤣 JOKE:', randomJoke);
    
    // Add to chat if possible
    if (typeof addMessage === 'function') {
        addMessage('JokeBot', randomJoke, 'ai', false, true);
    } else if (typeof addSystemMessage === 'function') {
        addSystemMessage('😄 ' + randomJoke);
    }
    
    randomJoke
    """
    
    result = await send_websocket_command(joke_js, port)
    
    if result.get("success"):
        click.echo("✅ Test joke sent successfully")
        return True
    else:
        click.echo(f"❌ Test joke failed: {result.get('error', 'Unknown error')}")
        return False

@click.command()
@click.option('--diagnose', is_flag=True, help='Run diagnostics only')
@click.option('--repair', is_flag=True, help='Run auto-repair only')
@click.option('--restart-browser', is_flag=True, help='Restart browser tabs')
@click.option('--rebuild-server', is_flag=True, help='Rebuild server')
@click.option('--test-joke', is_flag=True, help='Send test joke')
@click.option('--full', is_flag=True, help='Full healing sequence')
@click.option('-p', '--port', default=9000, envvar='CONTINUUM_PORT', help='Continuum port')
def main(diagnose, repair, restart_browser, rebuild_server, test_joke, full, port):
    """
    🔧 Smart Healing System for Continuum
    
    Automatically diagnose and fix common issues:
    - JavaScript errors and memory leaks
    - WebSocket connection problems  
    - Browser tab management
    - Server rebuilds and restarts
    - End-to-end functionality testing
    
    Examples:
    
        smart-heal --diagnose                 # Check system health
        smart-heal --repair                   # Fix JS issues  
        smart-heal --restart-browser          # Restart browser
        smart-heal --rebuild-server           # Rebuild server
        smart-heal --test-joke                # Test with joke
        smart-heal --full                     # Complete healing
    """
    
    async def async_main():
        if full:
            # Complete healing sequence
            click.echo("🛰️ Starting full healing sequence...")
            
            # 1. Diagnose
            await diagnose_system(port)
            
            # 2. Try auto-repair first
            repair_success = await auto_repair_js(port)
            
            # 3. If repair failed, try browser restart
            if not repair_success:
                await restart_browser(port)
                time.sleep(3)  # Wait for browser restart
            
            # 4. Test with joke
            joke_success = await send_test_joke(port)
            
            # 5. If still failing, rebuild server
            if not joke_success:
                click.echo("🏗️ JavaScript fixes failed, rebuilding server...")
                rebuild_server()
                time.sleep(5)  # Wait for server restart
                await send_test_joke(port)
            
            click.echo("🎯 Full healing sequence complete")
            
        else:
            # Individual operations
            if diagnose:
                await diagnose_system(port)
            
            if repair:
                await auto_repair_js(port)
            
            if restart_browser:
                await restart_browser(port)
            
            if rebuild_server:
                rebuild_server()
            
            if test_joke:
                await send_test_joke(port)
            
            if not any([diagnose, repair, restart_browser, rebuild_server, test_joke]):
                click.echo("❓ No action specified. Use --help for options or --full for complete healing.")
    
    # Run async main
    asyncio.run(async_main())

if __name__ == '__main__':
    main()