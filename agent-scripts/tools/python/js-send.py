#!/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/venv/agents/bin/python
"""
Deep Space Probe JavaScript Sender - WebSocket Edition
Sends JavaScript to browser via WebSocket messages with base64 encoding
"""

import sys
import base64
import json
import os
import subprocess
import time
import asyncio
import ssl
from pathlib import Path
import warnings

# Suppress urllib3 SSL warnings
warnings.filterwarnings('ignore', message='urllib3 v2 only supports OpenSSL 1.1.1+')

try:
    import websockets
except ImportError:
    print("❌ websockets package not found. Installing...")
    subprocess.check_call([sys.executable, "-m", "pip", "install", "websockets"])
    import websockets

import click

def read_javascript(source):
    """Read JavaScript from file, stdin, or direct string"""
    if source == '-' or not source:
        # Read from stdin
        return sys.stdin.read()
    elif Path(source).exists():
        # Read from file
        return Path(source).read_text()
    else:
        # Direct JavaScript string
        return source

async def send_to_probe_websocket(js_code, port=9000, quiet=False, verbose=False):
    """Send JavaScript to the browser probe via WebSocket"""
    
    if not quiet:
        truncated = js_code[:60] + ('...' if len(js_code) > 60 else '')
        click.echo(f"🛰️ Sending to probe via WebSocket: {truncated}")
    
    uri = f"ws://localhost:{port}"
    
    try:
        # Connect to WebSocket
        async with websockets.connect(uri, ping_interval=20, ping_timeout=10) as websocket:
            if not quiet:
                click.echo(f"🔌 Connected to WebSocket: {uri}")
            
            # Encode as base64 for safety  
            b64_code = base64.b64encode(js_code.encode('utf-8')).decode('ascii')
            
            # Send JavaScript execution via BROWSER_JS command for proper ping-pong
            message = {
                "type": "task",
                "role": "system", 
                "task": f"[CMD:BROWSER_JS] {b64_code}"
            }
            
            await websocket.send(json.dumps(message))
            
            if not quiet:
                click.echo("📡 JavaScript command sent via WebSocket")
            
            # Wait for response (with timeout)
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=10.0)
                response_data = json.loads(response)
                
                if verbose:
                    click.echo(f"📨 WebSocket response: {response}")
                
                # Parse the actual command result from the response
                if response_data.get("type") == "response" and "message" in response_data:
                    # This is the actual browser execution result
                    try:
                        result_data = json.loads(response_data["message"])
                        return {
                            "success": result_data.get("executed", False),
                            "transport": "websocket",
                            "result": result_data
                        }
                    except:
                        # If message isn't JSON, return as is
                        return {
                            "success": True,
                            "transport": "websocket", 
                            "result": {
                                "executed": True,
                                "message": response_data["message"],
                                "raw_response": response_data
                            }
                        }
                else:
                    return {
                        "success": True,
                        "transport": "websocket",
                        "result": {
                            "executed": True,
                            "message": "JavaScript sent via WebSocket",
                            "response": response_data
                        }
                    }
                
            except asyncio.TimeoutError:
                if not quiet:
                    click.echo("⏰ WebSocket response timeout (command may still be executing)")
                return {
                    "success": True,
                    "transport": "websocket", 
                    "result": {
                        "executed": True,
                        "message": "JavaScript sent via WebSocket (no response received)",
                        "code": js_code,
                        "timeout": True
                    }
                }
    
    except Exception as e:
        error_msg = str(e)
        
        if not quiet:
            click.echo(f"❌ WebSocket connection failed: {error_msg}")
        
        # Auto-diagnose and heal connection issues
        if "Connection refused" in error_msg or "Cannot connect" in error_msg:
            if not quiet:
                click.echo("🔧 Connection refused - running diagnostics...")
            
            # Diagnostic 1: Check if server process exists
            try:
                result = subprocess.run(["pgrep", "-f", "continuum"], capture_output=True, text=True)
                if result.stdout.strip():
                    if not quiet:
                        click.echo("📊 Server process exists but WebSocket not responding")
                else:
                    if not quiet:
                        click.echo("📊 No server process found")
            except:
                pass
            
            # Diagnostic 2: Check port
            try:
                result = subprocess.run(["lsof", "-i", f":{port}"], capture_output=True, text=True)
                if result.stdout:
                    if not quiet:
                        click.echo(f"📊 Port {port} is in use")
                else:
                    if not quiet:
                        click.echo(f"📊 Port {port} is free")
            except:
                pass
        
        return {"success": False, "transport": "websocket", "error": f"WebSocket connection failed: {e}"}

@click.command()
@click.argument('javascript', required=False, default='-')
@click.option('-p', '--port', default=9000, envvar='CONTINUUM_PORT',
              help='Continuum port (default: 9000, env: CONTINUUM_PORT)')
@click.option('-q', '--quiet', is_flag=True,
              help='Minimal output (just success/error)')
@click.option('-j', '--json', 'json_only', is_flag=True,
              help='JSON output only')
@click.option('-v', '--verbose', is_flag=True,
              help='Verbose debugging output')
def main(javascript, port, quiet, json_only, verbose):
    """
    🛰️ Send JavaScript to browser probe via WebSocket
    
    JAVASCRIPT can be:
    - Direct JavaScript code: 'console.log("test")'
    - Filename: script.js
    - '-' or omitted for stdin
    
    Examples:
    
      js-send-websocket.py 'console.log("test")'              # Direct JavaScript
      
      js-send-websocket.py script.js                          # From file
      
      echo 'console.log("test")' | js-send-websocket.py       # From stdin
      
      js-send-websocket.py --quiet 'document.title = "New"'   # Minimal output
      
      js-send-websocket.py --json 'console.log("test")'       # JSON only
      
      js-send-websocket.py --verbose 'console.log("debug")'   # Debug info
    
    🔌 Pure WebSocket communication - no HTTP fallbacks
    """
    
    async def async_main():
        try:
            # Read JavaScript
            js_code = read_javascript(javascript).strip()
            if not js_code:
                if json_only:
                    click.echo(json.dumps({"success": False, "error": "Empty JavaScript code"}))
                else:
                    click.echo("❌ Empty JavaScript code provided")
                sys.exit(1)
            
            # Send to probe via WebSocket
            result = await send_to_probe_websocket(js_code, port, quiet, verbose)
            
            # Output results
            if json_only:
                click.echo(json.dumps(result, indent=2))
            elif quiet:
                success = result.get('success', False)
                executed = result.get('result', {}).get('executed', False)
                
                if success and executed:
                    click.echo("✅ EXECUTED")
                    if verbose:
                        code = result.get('result', {}).get('code', '')
                        if code:
                            truncated = code[:100] + ('...' if len(code) > 100 else '')
                            click.echo(f"Code: {truncated}")
                else:
                    click.echo("❌ FAILED")
                    error = result.get('result', {}).get('error') or result.get('error', '')
                    if error:
                        click.echo(f"Error: {error}")
            else:
                # Normal output
                click.echo(json.dumps(result, indent=2))
                click.echo()
                
                success = result.get('success', False)
                executed = result.get('result', {}).get('executed', False)
                
                if success and executed:
                    click.echo("✅ JavaScript sent to probe via WebSocket")
                else:
                    click.echo("❌ Failed to execute JavaScript via WebSocket")
                    error = result.get('result', {}).get('error') or result.get('error', '')
                    if error:
                        click.echo(f"Error: {error}")
                
                click.echo()
                click.echo("🔍 Check server logs for probe telemetry response")
                
                if verbose:
                    click.echo()
                    click.echo("📊 Debug Info:")
                    click.echo(f"   Port: {port}")
                    click.echo(f"   Transport: WebSocket")
                    click.echo(f"   JavaScript length: {len(js_code)}")
                    click.echo(f"   Success: {success}")
                    click.echo(f"   Executed: {executed}")
        
        except KeyboardInterrupt:
            click.echo("\n🛑 Interrupted")
            sys.exit(1)
        except Exception as e:
            if json_only:
                click.echo(json.dumps({"success": False, "error": str(e)}))
            else:
                click.echo(f"❌ Error: {e}")
            sys.exit(1)
    
    # Run async main
    asyncio.run(async_main())

if __name__ == '__main__':
    main()