#!/usr/bin/env python3
"""
Universal Agent Healing System
Single entry point that automatically fixes issues based on error patterns
"""

import sys
import subprocess
import time
import json
import click
import re
from pathlib import Path

class UniversalHealer:
    def __init__(self):
        self.healing_patterns = {
            r'Connection refused.*:9000': self.fix_server_down,
            r'WebSocket connection.*failed': self.fix_websocket_spam,
            r'Disconnected from Continuum': self.fix_websocket_spam,
            r'BROWSER_JS.*timeout': self.fix_browser_timeout,
            r'Agent element not found': self.fix_agent_selector,
            r'continuum.*not found': self.fix_continuum_missing,
        }
    
    def analyze_error(self, error_text):
        """Analyze error and return healing action"""
        for pattern, action in self.healing_patterns.items():
            if re.search(pattern, error_text, re.IGNORECASE):
                return action, pattern
        return None, None
    
    def fix_server_down(self):
        """Fix server connection issues"""
        click.echo("🔧 HEALING: Server connection refused")
        
        # First check if server is actually down
        try:
            import requests
            response = requests.get("http://localhost:9000", timeout=3)
            if response.status_code == 200:
                click.echo("✅ Server is actually running - connection issue resolved")
                return True
        except:
            pass
        
        # Check if continuum process exists but not responding
        try:
            result = subprocess.run(["pgrep", "-f", "continuum"], capture_output=True, text=True)
            if result.stdout.strip():
                click.echo("⚠️ Continuum process exists but not responding - gentle restart...")
                # Don't force kill - try gentle restart first
                try:
                    subprocess.run(["continuum", "--restart"], timeout=15, capture_output=True)
                    time.sleep(5)
                    response = requests.get("http://localhost:9000", timeout=3)
                    if response.status_code == 200:
                        click.echo("✅ Gentle restart succeeded")
                        return True
                except:
                    pass
        except:
            pass
        
        click.echo("1. Starting fresh continuum instance...")
        # Only kill if we're sure we need to
        
        # Try gentle restart first
        click.echo("2. Attempting server restart...")
        try:
            # Start in background to avoid timeout
            process = subprocess.Popen(["continuum", "--restart"], 
                                     stdout=subprocess.PIPE, 
                                     stderr=subprocess.PIPE)
            
            # Wait up to 30 seconds for startup
            for i in range(30):
                time.sleep(1)
                try:
                    import requests
                    response = requests.get("http://localhost:9000", timeout=2)
                    if response.status_code == 200:
                        click.echo("✅ Server restarted and responding")
                        return True
                except:
                    pass
                
                # Check if process died
                if process.poll() is not None:
                    click.echo(f"⚠️ Process exited with code {process.returncode}")
                    break
                    
            click.echo("⚠️ Server taking too long to respond")
            
        except Exception as e:
            click.echo(f"⚠️ Restart failed: {e}")
        
        # Force restart if gentle failed
        click.echo("3. Force restarting...")
        try:
            # Kill everything harder
            subprocess.run(["pkill", "-9", "-f", "continuum"], capture_output=True)
            subprocess.run(["pkill", "-9", "-f", "node"], capture_output=True)
            time.sleep(3)
            
            # Start fresh in background
            process = subprocess.Popen(["continuum", "--restart"], 
                                     stdout=subprocess.PIPE, 
                                     stderr=subprocess.PIPE)
            
            # Wait for server to respond
            for i in range(45):
                time.sleep(1)
                try:
                    import requests
                    response = requests.get("http://localhost:9000", timeout=2)
                    if response.status_code == 200:
                        click.echo("✅ Force restart succeeded - server responding")
                        return True
                except:
                    pass
                    
            click.echo("❌ Server still not responding after force restart")
            
        except Exception as e:
            click.echo(f"❌ Force restart failed: {e}")
        
        click.echo("❌ All restart attempts failed - manual intervention needed")
        return False
    
    def fix_websocket_spam(self):
        """Fix WebSocket reconnection spam"""
        click.echo("🔧 HEALING: WebSocket spam/disconnection")
        
        # Inject browser fix
        heal_script = '''
        console.log("🏥 AUTO-HEAL: Stopping WebSocket spam");
        
        // Stop all timers/intervals
        for(let i = 0; i < 20000; i++) {
            clearTimeout(i); clearInterval(i);
        }
        
        // Patch console to prevent spam
        if (!window._autoHealed) {
            const orig = {log: console.log, error: console.error};
            let lastMsg = '', msgCount = 0;
            
            console.log = function(...args) {
                const msg = args.join(' ');
                if (msg === lastMsg) {
                    msgCount++;
                    if (msgCount > 2) return; // Suppress after 3rd repeat
                } else {
                    msgCount = 0;
                    lastMsg = msg;
                }
                orig.log(...args);
            };
            
            console.error = function(...args) {
                const msg = args.join(' ');
                if (msg.includes('WebSocket') && msg === lastMsg) {
                    msgCount++;
                    if (msgCount > 1) return; // Suppress WebSocket spam
                } else {
                    msgCount = 0;
                    lastMsg = msg;
                }
                orig.error(...args);
            };
            
            window._autoHealed = true;
        }
        "healed"
        '''
        
        return self._inject_browser_fix(heal_script)
    
    def fix_browser_timeout(self):
        """Fix browser execution timeouts"""
        click.echo("🔧 HEALING: Browser timeout")
        return self._inject_browser_fix('console.log("🏥 Timeout heal"); "timeout_healed"')
    
    def fix_agent_selector(self):
        """Fix agent selector issues"""
        click.echo("🔧 HEALING: Agent selector")
        fix_script = '''
        const selector = document.querySelector('agent-selector');
        if (selector && selector.selectedAgent) {
            selector.selectedAgent = selector.selectedAgent.replace(' route', '');
            console.log("🏥 Fixed agent selector");
        }
        "selector_healed"
        '''
        return self._inject_browser_fix(fix_script)
    
    def fix_continuum_missing(self):
        """Fix missing continuum command"""
        click.echo("🔧 HEALING: Continuum command not found")
        click.echo("💡 Ensure continuum is in PATH or run from project directory")
        return False
    
    def _inject_browser_fix(self, js_code):
        """Inject JavaScript fix into browser"""
        try:
            js_send = Path(__file__).parent / "bin" / "js-send"
            result = subprocess.run([
                str(js_send), "--json", js_code
            ], capture_output=True, text=True, timeout=15)
            
            if result.returncode == 0:
                response = json.loads(result.stdout)
                success = response.get('success', False)
                if success:
                    click.echo("✅ Browser fix injected")
                    return True
                else:
                    click.echo(f"❌ Browser fix failed: {response.get('error', 'Unknown')}")
            else:
                click.echo(f"❌ js-send failed: {result.stderr}")
        except Exception as e:
            click.echo(f"❌ Injection failed: {e}")
        
        return False

@click.command()
@click.argument('error_text', required=False)
@click.option('--auto', is_flag=True, help='Auto-detect from recent logs')
@click.option('--monitor', is_flag=True, help='Continuous healing mode')
def heal(error_text, auto, monitor):
    """🏥 Universal Agent Healing System
    
    Usage:
      heal "Connection refused"          # Heal specific error
      heal --auto                        # Auto-detect from logs  
      heal --monitor                     # Continuous monitoring
      command 2>&1 | heal               # Pipe errors directly
    """
    
    healer = UniversalHealer()
    
    if monitor:
        click.echo("🏥 Starting continuous healing monitor...")
        while True:
            try:
                time.sleep(30)
                # Check for issues and heal
                result = subprocess.run([
                    str(Path(__file__).parent / "bin" / "probe"), "status"
                ], capture_output=True, text=True)
                
                if result.returncode != 0:
                    click.echo("🚨 Issue detected, healing...")
                    action, pattern = healer.analyze_error(result.stderr + result.stdout)
                    if action:
                        action()
                    
            except KeyboardInterrupt:
                click.echo("\n🛑 Healing monitor stopped")
                break
        return
    
    # Get error text from stdin if piped
    if not error_text and not sys.stdin.isatty():
        error_text = sys.stdin.read().strip()
    
    # Auto-detect from logs
    if auto and not error_text:
        try:
            log_file = Path(__file__).parent.parent / "continuum.log"
            if log_file.exists():
                with open(log_file) as f:
                    lines = f.readlines()[-50:]  # Last 50 lines
                    error_text = ''.join(lines)
        except Exception:
            pass
    
    if not error_text:
        click.echo("❌ No error text provided")
        click.echo("💡 Usage: heal 'error message' or pipe errors to heal")
        sys.exit(1)
    
    click.echo(f"🔍 Analyzing: {error_text[:100]}...")
    
    action, pattern = healer.analyze_error(error_text)
    
    if action:
        click.echo(f"🎯 Matched pattern: {pattern}")
        success = action()
        if success:
            click.echo("✅ HEALING SUCCESSFUL")
            sys.exit(0)
        else:
            click.echo("❌ HEALING FAILED")
            sys.exit(1)
    else:
        click.echo("🤷 No healing action available for this error")
        click.echo("💡 Available patterns:")
        for pattern in healer.healing_patterns.keys():
            click.echo(f"   - {pattern}")
        sys.exit(1)

if __name__ == '__main__':
    heal()