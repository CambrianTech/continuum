#!/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/venv/agents/bin/python
"""
Deep Space Probe JavaScript Sender
Sends JavaScript to browser via base64-encoded WebSocket commands
"""

import sys
import base64
import json
import os
import subprocess
import time
from pathlib import Path
import warnings

# Suppress urllib3 SSL warnings
warnings.filterwarnings('ignore', message='urllib3 v2 only supports OpenSSL 1.1.1+')

import requests
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

def send_to_probe(js_code, port=9000, quiet=False, verbose=False):
    """Send JavaScript to the browser probe with auto-healing"""
    # Encode as base64
    b64_code = base64.b64encode(js_code.encode('utf-8')).decode('ascii')
    
    if not quiet:
        truncated = js_code[:60] + ('...' if len(js_code) > 60 else '')
        click.echo(f"🛰️ Sending to probe: {truncated}")
        if verbose:
            click.echo(f"📡 Base64: {b64_code}")
    
    # Send to Continuum
    payload = {
        "command": "BROWSER_JS",
        "params": b64_code,
        "encoding": "base64"
    }
    
    try:
        response = requests.post(
            f"http://localhost:{port}/connect",
            json=payload,
            timeout=10
        )
        return response.json()
    except requests.exceptions.RequestException as e:
        error_msg = str(e)
        
        # Auto-diagnose and heal connection issues
        if "Connection refused" in error_msg:
            if not quiet:
                click.echo("🔧 Connection refused - running diagnostics...")
            
            # Diagnostic 1: Check if server process exists
            try:
                result = subprocess.run(["pgrep", "-f", "continuum"], capture_output=True, text=True)
                if result.stdout.strip():
                    if not quiet:
                        click.echo("📊 Server process exists but not responding")
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
            
            # Auto-heal: Try restarting
            if not quiet:
                click.echo("🔧 Attempting auto-restart...")
            
            try:
                # Start continuum in background
                process = subprocess.Popen(["continuum", "--restart"], 
                                         stdout=subprocess.PIPE, 
                                         stderr=subprocess.PIPE)
                
                # Wait for startup
                for i in range(20):
                    time.sleep(2)
                    try:
                        test_response = requests.get(f"http://localhost:{port}", timeout=2)
                        if test_response.status_code == 200:
                            if not quiet:
                                click.echo("✅ Auto-restart successful - retrying joke...")
                            
                            # Retry the joke
                            response = requests.post(
                                f"http://localhost:{port}/connect",
                                json=payload,
                                timeout=10
                            )
                            return response.json()
                    except:
                        pass
                
                if not quiet:
                    click.echo("❌ Auto-restart failed - server not responding")
                    
            except Exception as e:
                if not quiet:
                    click.echo(f"❌ Auto-restart error: {e}")
        
        return {"success": False, "error": f"Connection failed: {e}"}

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
    🛰️ Send JavaScript to browser probe via base64 encoding
    
    JAVASCRIPT can be:
    - Direct JavaScript code: 'console.log("test")'
    - Filename: script.js
    - '-' or omitted for stdin
    
    Examples:
    
      js-send.py 'console.log("test")'              # Direct JavaScript
      
      js-send.py script.js                          # From file
      
      echo 'console.log("test")' | js-send.py       # From stdin
      
      js-send.py --quiet 'document.title = "New"'   # Minimal output
      
      js-send.py --json 'console.log("test")'       # JSON only
      
      js-send.py --verbose 'console.log("debug")'   # Debug info
    
    📡 All JavaScript is automatically base64 encoded for safe transmission
    """
    
    try:
        # Read JavaScript
        js_code = read_javascript(javascript).strip()
        if not js_code:
            if json_only:
                click.echo(json.dumps({"success": False, "error": "Empty JavaScript code"}))
            else:
                click.echo("❌ Empty JavaScript code provided")
            sys.exit(1)
        
        # Send to probe
        result = send_to_probe(js_code, port, quiet, verbose)
        
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
                click.echo("✅ JavaScript sent to probe successfully")
            else:
                click.echo("❌ Failed to execute JavaScript")
                error = result.get('result', {}).get('error') or result.get('error', '')
                if error:
                    click.echo(f"Error: {error}")
            
            click.echo()
            click.echo("🔍 Check server logs for probe telemetry response")
            
            if verbose:
                click.echo()
                click.echo("📊 Debug Info:")
                click.echo(f"   Port: {port}")
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

if __name__ == '__main__':
    main()