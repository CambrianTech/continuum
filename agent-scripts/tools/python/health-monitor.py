#!/usr/bin/env python3
"""
Agent Health Monitor - Self-Healing System
Automatically detects and fixes common probe issues
"""

import time
import json
import subprocess
import sys
from pathlib import Path
import click
import requests
from datetime import datetime

class ProbeHealthMonitor:
    def __init__(self, port=9000):
        self.port = port
        self.last_health_check = 0
        self.consecutive_failures = 0
        self.healing_actions = {
            'websocket_spam': self.fix_websocket_spam,
            'server_crash': self.restart_server,
            'connection_refused': self.wait_for_server,
        }
        
    def diagnose_issue(self):
        """Diagnose what's wrong with the probe"""
        try:
            response = requests.get(f"http://localhost:{self.port}", timeout=5)
            if response.status_code == 200:
                return "healthy"
        except requests.exceptions.ConnectionError:
            return "connection_refused"
        except requests.exceptions.Timeout:
            return "server_slow"
        except Exception as e:
            return f"unknown_error: {e}"
    
    def fix_websocket_spam(self):
        """Fix WebSocket reconnection spam"""
        click.echo("🔧 Healing: WebSocket spam detected")
        
        # Inject spam fix via agent system
        fix_js = '''
        console.log("🏥 HEALTH MONITOR: Fixing WebSocket spam");
        
        // Stop all reconnection attempts
        for(let i = 0; i < 10000; i++) {
            clearTimeout(i);
            clearInterval(i);
        }
        
        // Rate limit console output
        if (!window._healthMonitorFixed) {
            const origLog = console.log;
            const origError = console.error;
            let lastSpamLog = 0;
            
            console.log = function(...args) {
                const msg = args.join(' ');
                if (msg.includes('Disconnected') || msg.includes('WebSocket')) {
                    if (Date.now() - lastSpamLog > 10000) {
                        origLog("🔌 Connection issues (health monitor active)");
                        lastSpamLog = Date.now();
                    }
                } else {
                    origLog(...args);
                }
            };
            
            console.error = function(...args) {
                const msg = args.join(' ');
                if (msg.includes('WebSocket')) {
                    // Silent for WebSocket errors
                } else {
                    origError(...args);
                }
            };
            
            window._healthMonitorFixed = true;
        }
        
        "spam_healed"
        '''
        
        # Use agent system to inject fix
        js_send = Path(__file__).parent / "bin" / "js-send"
        try:
            result = subprocess.run([
                sys.executable, 
                str(Path(__file__).parent / "bin" / "run-with-venv.py"),
                "js-send.py",
                "--json",
                fix_js
            ], capture_output=True, text=True, timeout=15)
            
            if result.returncode == 0:
                response = json.loads(result.stdout)
                if response.get('success'):
                    click.echo("✅ WebSocket spam healed")
                    return True
        except Exception as e:
            click.echo(f"❌ Healing failed: {e}")
        
        return False
    
    def restart_server(self):
        """Restart the continuum server"""
        click.echo("🏥 Healing: Restarting crashed server")
        try:
            result = subprocess.run(["continuum", "--restart"], 
                                  capture_output=True, text=True, timeout=30)
            if result.returncode == 0:
                click.echo("✅ Server restarted")
                return True
        except subprocess.TimeoutExpired:
            click.echo("⏰ Server restart taking longer than expected")
        except Exception as e:
            click.echo(f"❌ Restart failed: {e}")
        return False
    
    def wait_for_server(self):
        """Wait for server to come online"""
        click.echo("⏳ Waiting for server to start...")
        for i in range(12):  # 60 second max wait
            time.sleep(5)
            if self.diagnose_issue() == "healthy":
                click.echo("✅ Server is healthy")
                return True
            click.echo(f"⏳ Still waiting... ({i+1}/12)")
        return False
    
    def health_check(self):
        """Perform comprehensive health check"""
        click.echo(f"🏥 Health check at {datetime.now().strftime('%H:%M:%S')}")
        
        issue = self.diagnose_issue()
        
        if issue == "healthy":
            self.consecutive_failures = 0
            click.echo("✅ Probe is healthy")
            return True
        
        self.consecutive_failures += 1
        click.echo(f"🚨 Issue detected: {issue} (failure #{self.consecutive_failures})")
        
        # Auto-heal based on issue type
        if issue in self.healing_actions:
            if self.healing_actions[issue]():
                self.consecutive_failures = 0
                return True
        
        return False
    
    def monitor_loop(self, interval=30):
        """Continuous monitoring loop"""
        click.echo("🏥 Starting probe health monitor...")
        click.echo(f"📊 Monitoring port {self.port} every {interval}s")
        
        while True:
            try:
                self.health_check()
                time.sleep(interval)
            except KeyboardInterrupt:
                click.echo("\n🛑 Health monitor stopped")
                break
            except Exception as e:
                click.echo(f"❌ Monitor error: {e}")
                time.sleep(interval)

@click.group()
def cli():
    """🏥 Probe Health Monitor - Self-Healing Agent System"""
    pass

@cli.command()
@click.option('--port', default=9000, help='Continuum port to monitor')
@click.option('--interval', default=30, help='Check interval in seconds')
def monitor(port, interval):
    """Start continuous health monitoring"""
    monitor = ProbeHealthMonitor(port)
    monitor.monitor_loop(interval)

@cli.command()
@click.option('--port', default=9000, help='Continuum port to check')
def check(port):
    """Perform single health check"""
    monitor = ProbeHealthMonitor(port)
    healthy = monitor.health_check()
    sys.exit(0 if healthy else 1)

@cli.command()
@click.option('--port', default=9000, help='Continuum port')
def heal(port):
    """Attempt to heal detected issues"""
    monitor = ProbeHealthMonitor(port)
    issue = monitor.diagnose_issue()
    
    if issue == "healthy":
        click.echo("✅ No issues detected")
        return
    
    click.echo(f"🔧 Attempting to heal: {issue}")
    if issue in monitor.healing_actions:
        success = monitor.healing_actions[issue]()
        if success:
            click.echo("✅ Healing successful")
        else:
            click.echo("❌ Healing failed")
    else:
        click.echo(f"🤷 No healing action available for: {issue}")

if __name__ == '__main__':
    cli()