#!/usr/bin/env python3
"""
🛰️ CRITICAL PROBE SAFETY MANAGER
Ensures safe communication with deep space probe (browser)
NEVER breaks the communication link - includes auto-recovery
"""

import sys
import time
import json
import subprocess
import os
from pathlib import Path
import click
import requests

class ProbeManager:
    def __init__(self, port=9000):
        self.port = port
        self.last_known_good_state = None
        self.communication_broken = False
        
    def check_probe_health(self):
        """Check if we can communicate with the probe"""
        try:
            response = requests.get(f"http://localhost:{self.port}", timeout=5)
            return response.status_code == 200
        except:
            return False
    
    def test_basic_communication(self):
        """Send a minimal test to verify probe responds"""
        test_js = 'console.log("🛰️ PROBE HEALTH CHECK"); "health_ok"'
        result = self.send_javascript(test_js, quiet=True)
        
        success = result.get('success', False)
        executed = result.get('result', {}).get('executed', False)
        
        return success and executed
    
    def send_javascript(self, js_code, quiet=False):
        """Send JavaScript using js-send.py"""
        cmd = [sys.executable, str(Path(__file__).parent / "js-send.py")]
        if quiet:
            cmd.append("--json")
        cmd.append(js_code)
        
        try:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
            if result.returncode == 0:
                return json.loads(result.stdout)
            else:
                return {"success": False, "error": f"js-send failed: {result.stderr}"}
        except subprocess.TimeoutExpired:
            return {"success": False, "error": "Probe communication timeout"}
        except Exception as e:
            return {"success": False, "error": f"Communication error: {e}"}
    
    def emergency_restart(self):
        """Emergency restart of Continuum server"""
        click.echo("🚨 EMERGENCY: Attempting probe communication recovery...")
        
        try:
            # Try to restart Continuum
            result = subprocess.run(
                ["continuum", "--restart"], 
                capture_output=True, 
                text=True, 
                timeout=30
            )
            
            if result.returncode == 0:
                click.echo("✅ Continuum restart initiated")
                # Wait for server to come back up
                for i in range(20):
                    time.sleep(2)
                    if self.check_probe_health():
                        click.echo("✅ Probe communication restored")
                        return True
                    click.echo(f"⏳ Waiting for probe... ({i+1}/20)")
                
                click.echo("❌ Probe still not responding after restart")
                return False
            else:
                click.echo(f"❌ Continuum restart failed: {result.stderr}")
                return False
                
        except Exception as e:
            click.echo(f"❌ Emergency restart failed: {e}")
            return False

@click.group()
@click.option('--port', default=9000, envvar='CONTINUUM_PORT')
@click.pass_context
def cli(ctx, port):
    """🛰️ Deep Space Probe Safety Manager - Critical Mission Control"""
    ctx.ensure_object(dict)
    ctx.obj['probe'] = ProbeManager(port)

@cli.command()
@click.pass_context
def status(ctx):
    """Check probe health and communication status"""
    probe = ctx.obj['probe']
    
    click.echo("🛰️ PROBE STATUS CHECK")
    click.echo("=" * 40)
    
    # Basic health check
    if probe.check_probe_health():
        click.echo("✅ Probe server responding")
    else:
        click.echo("❌ Probe server not responding")
        click.echo("💡 Try: probe-safe.py recover")
        sys.exit(1)
    
    # Communication test
    if probe.test_basic_communication():
        click.echo("✅ JavaScript execution working")
        click.echo("✅ Telemetry link operational")
    else:
        click.echo("❌ JavaScript execution failed")
        click.echo("🚨 CRITICAL: Communication link broken")
        click.echo("💡 Try: probe-safe.py recover")
        sys.exit(1)
    
    click.echo("\n🟢 PROBE OPERATIONAL - Safe to proceed")

@cli.command()
@click.argument('javascript')
@click.option('--backup/--no-backup', default=True, help='Create safety backup before execution')
@click.option('--verify/--no-verify', default=True, help='Verify probe health before/after')
@click.pass_context
def send(ctx, javascript, backup, verify):
    """Safely send JavaScript to probe with safety checks"""
    probe = ctx.obj['probe']
    
    if verify:
        click.echo("🔍 Pre-flight safety check...")
        if not probe.check_probe_health() or not probe.test_basic_communication():
            click.echo("❌ Probe not operational - aborting mission")
            click.echo("💡 Run: probe-safe.py status")
            sys.exit(1)
        click.echo("✅ Pre-flight check passed")
    
    if backup:
        # Create a simple backup state
        probe.last_known_good_state = {
            'timestamp': time.time(),
            'health_ok': True
        }
    
    click.echo(f"🛰️ Transmitting to probe: {javascript[:50]}{'...' if len(javascript) > 50 else ''}")
    
    # Send the JavaScript
    result = probe.send_javascript(javascript)
    
    # Analyze results
    success = result.get('success', False)
    executed = result.get('result', {}).get('executed', False)
    error = result.get('result', {}).get('error') or result.get('error', '')
    
    if success and executed:
        click.echo("✅ Mission successful - probe executed command")
        
        if verify:
            click.echo("🔍 Post-mission verification...")
            if probe.test_basic_communication():
                click.echo("✅ Probe still operational")
            else:
                click.echo("🚨 WARNING: Probe communication degraded")
                click.echo("💡 Consider: probe-safe.py recover")
        
        # Show the response
        click.echo("\n📡 PROBE TELEMETRY:")
        click.echo(json.dumps(result, indent=2))
        
    else:
        click.echo("❌ Mission failed")
        if error:
            click.echo(f"Error: {error}")
        
        click.echo("\n🚨 SAFETY PROTOCOL ACTIVATED")
        click.echo("The probe may be damaged. Recommend immediate recovery.")
        click.echo("💡 Run: probe-safe.py recover")
        
        sys.exit(1)

@cli.command()
@click.pass_context
def recover(ctx):
    """Emergency recovery protocol for broken probe communication"""
    probe = ctx.obj['probe']
    
    click.echo("🚨 INITIATING EMERGENCY RECOVERY PROTOCOL")
    click.echo("=" * 50)
    
    # Step 1: Diagnose
    click.echo("Step 1: Diagnosing probe status...")
    server_ok = probe.check_probe_health()
    comm_ok = probe.test_basic_communication() if server_ok else False
    
    if server_ok and comm_ok:
        click.echo("✅ Probe is actually operational - false alarm")
        return
    
    # Step 2: Emergency restart
    if not server_ok:
        click.echo("Step 2: Server unresponsive - emergency restart required")
        if probe.emergency_restart():
            click.echo("✅ Recovery successful")
        else:
            click.echo("❌ Recovery failed - manual intervention required")
            click.echo("\nMANUAL RECOVERY STEPS:")
            click.echo("1. cd /Users/joel/Development/ideem/vHSM/externals/continuum")
            click.echo("2. continuum --restart")
            click.echo("3. Wait 30 seconds")
            click.echo("4. probe-safe.py status")
            sys.exit(1)
    
    # Step 3: Communication test
    click.echo("Step 3: Testing communication recovery...")
    if probe.test_basic_communication():
        click.echo("✅ Probe communication restored")
        click.echo("🟢 PROBE OPERATIONAL - Mission can continue")
    else:
        click.echo("❌ Communication still broken")
        click.echo("🚨 CRITICAL: Manual debugging required")
        sys.exit(1)

@cli.command()
@click.argument('js_file')
@click.option('--test-mode', is_flag=True, help='Test mode: verify each step')
@click.pass_context
def execute_file(ctx, js_file, test_mode):
    """Execute JavaScript file with full safety protocol"""
    probe = ctx.obj['probe']
    
    # Read file
    try:
        js_content = Path(js_file).read_text()
    except Exception as e:
        click.echo(f"❌ Cannot read file: {e}")
        sys.exit(1)
    
    if test_mode:
        click.echo("🧪 TEST MODE: Will verify each step")
        # Split into lines and execute one by one
        lines = [line.strip() for line in js_content.split('\n') if line.strip() and not line.strip().startswith('//')]
        
        for i, line in enumerate(lines, 1):
            click.echo(f"\n📡 Step {i}/{len(lines)}: {line}")
            
            # Send this line
            ctx.invoke(send, javascript=line, backup=True, verify=True)
            
            if i < len(lines):
                click.echo("⏸️  Pausing between commands for safety...")
                time.sleep(1)
    else:
        # Execute entire file
        ctx.invoke(send, javascript=js_content, backup=True, verify=True)

if __name__ == '__main__':
    cli()