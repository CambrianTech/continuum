#!/usr/bin/env python3

"""
JTAG Async Probe - Promise-based debugging for AI autonomous development
========================================================================
Non-blocking, async-first debugging utilities that integrate with Continuum's
command system for real-time widget analysis and system health monitoring.
"""

import asyncio
import json
import os
import time
from pathlib import Path
from typing import Dict, Any, Optional, Callable
import subprocess

class JtagAsyncProbe:
    """Promise-based JTAG debugging interface"""
    
    def __init__(self, continuum_path: str = "./continuum"):
        self.continuum_path = continuum_path
        self.session_logs_dir = self._find_session_logs_dir()
    
    def _find_session_logs_dir(self) -> Optional[str]:
        """Find the current session logs directory"""
        sessions_base = ".continuum/sessions/user/shared"
        if os.path.exists(sessions_base):
            sessions = [d for d in os.listdir(sessions_base) if d.startswith("development-shared-")]
            if sessions:
                latest = max(sessions, key=lambda x: os.path.getmtime(os.path.join(sessions_base, x)))
                return os.path.join(sessions_base, latest, "logs")
        return None
    
    async def execute_js_async(self, javascript_code: str, timeout: int = 5) -> Dict[str, Any]:
        """Execute JavaScript in browser asynchronously"""
        try:
            process = await asyncio.create_subprocess_exec(
                self.continuum_path, 'execute', '--javascript', javascript_code,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            
            stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=timeout)
            
            if process.returncode != 0:
                return {"success": False, "error": stderr.decode().strip()}
            
            output = stdout.decode().strip()
            if output:
                try:
                    return json.loads(output)
                except json.JSONDecodeError:
                    return {"success": True, "output": output}
            return {"success": True, "output": "No output"}
            
        except asyncio.TimeoutError:
            if 'process' in locals():
                process.kill()
                await process.wait()
            return {"success": False, "error": f"JavaScript execution timed out after {timeout}s"}
        except Exception as e:
            return {"success": False, "error": f"Execution error: {str(e)}"}
    
    async def analyze_widgets_async(self) -> Dict[str, Any]:
        """Analyze widget rendering states asynchronously"""
        js_code = """
        const widgets = Array.from(document.querySelectorAll('*')).filter(el => el.tagName.includes('-'));
        const analysis = widgets.map(widget => ({
            tagName: widget.tagName.toLowerCase(),
            hasShadowRoot: !!widget.shadowRoot,
            shadowContentLength: widget.shadowRoot ? widget.shadowRoot.innerHTML.length : 0,
            visible: widget.offsetWidth > 0 && widget.offsetHeight > 0,
            hasTextContent: (widget.textContent?.trim().length || 0) > 0,
            hasShadowContent: (widget.shadowRoot?.textContent?.trim().length || 0) > 0,
            classList: Array.from(widget.classList),
            boundingRect: {
                width: widget.getBoundingClientRect().width,
                height: widget.getBoundingClientRect().height,
                x: widget.getBoundingClientRect().x,
                y: widget.getBoundingClientRect().y
            }
        }));
        
        const summary = {
            totalWidgets: analysis.length,
            workingWidgets: analysis.filter(w => w.hasShadowRoot && w.shadowContentLength > 100).length,
            emptyWidgets: analysis.filter(w => w.hasShadowRoot && w.shadowContentLength < 50).length,
            visibleWidgets: analysis.filter(w => w.visible).length,
            brokenWidgets: analysis.filter(w => !w.hasShadowRoot).length
        };
        
        return { analysis, summary };
        """
        
        return await self.execute_js_async(js_code)
    
    async def health_check_async(self) -> Dict[str, Any]:
        """System health check asynchronously"""
        js_code = """
        const health = {
            widgets: {
                total: document.querySelectorAll('*').length,
                custom: document.querySelectorAll('*[tagName*="-"]').length,
                withShadow: Array.from(document.querySelectorAll('*')).filter(el => el.shadowRoot).length
            },
            errors: window.continuumErrorCount || 0,
            connected: window.continuum?.isConnected() || false,
            sessionId: window.continuum?.sessionId || 'unknown',
            url: window.location.href,
            timestamp: new Date().toISOString(),
            performance: {
                memory: performance.memory ? {
                    used: Math.round(performance.memory.usedJSHeapSize / 1024 / 1024),
                    total: Math.round(performance.memory.totalJSHeapSize / 1024 / 1024)
                } : null,
                timing: performance.timing ? {
                    loadTime: performance.timing.loadEventEnd - performance.timing.navigationStart
                } : null
            }
        };
        return health;
        """
        
        return await self.execute_js_async(js_code)
    
    async def take_screenshot_async(self, selector: Optional[str] = None) -> Dict[str, Any]:
        """Take screenshot asynchronously"""
        args = ['screenshot']
        if selector:
            args.extend(['--selector', selector])
        
        try:
            process = await asyncio.create_subprocess_exec(
                self.continuum_path, *args,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            
            stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=10)
            
            if process.returncode != 0:
                return {"success": False, "error": stderr.decode().strip()}
            
            # Try to parse JSON response
            try:
                return json.loads(stdout.decode().strip())
            except json.JSONDecodeError:
                return {"success": True, "output": stdout.decode().strip()}
                
        except asyncio.TimeoutError:
            return {"success": False, "error": "Screenshot timed out"}
        except Exception as e:
            return {"success": False, "error": f"Screenshot error: {str(e)}"}
    
    async def follow_logs_async(self, level: Optional[str] = None, filter_func: Optional[Callable] = None):
        """Follow logs asynchronously with filtering"""
        if not self.session_logs_dir:
            yield {"error": "No active session found"}
            return
        
        if level:
            log_file = f"{self.session_logs_dir}/browser.{level}.json"
        else:
            log_file = f"{self.session_logs_dir}/browser.log"
        
        if not os.path.exists(log_file):
            yield {"error": f"Log file not found: {log_file}"}
            return
        
        # Start from end of file
        with open(log_file, 'r') as f:
            f.seek(0, 2)  # Seek to end
            
            try:
                while True:
                    line = f.readline()
                    if line:
                        if not filter_func or filter_func(line):
                            yield {"line": line.strip(), "timestamp": time.time()}
                    else:
                        # No new data, wait briefly
                        await asyncio.sleep(0.1)
            except asyncio.CancelledError:
                yield {"info": "Log following cancelled"}
    
    def get_recent_logs(self, lines: int = 20, level: Optional[str] = None) -> Dict[str, Any]:
        """Get recent logs synchronously"""
        if not self.session_logs_dir:
            return {"success": False, "error": "No active session found"}
        
        if level:
            log_file = f"{self.session_logs_dir}/browser.{level}.json"
        else:
            log_file = f"{self.session_logs_dir}/browser.log"
        
        if not os.path.exists(log_file):
            return {"success": False, "error": f"Log file not found: {log_file}"}
        
        try:
            result = subprocess.run(['tail', f'-{lines}', log_file], 
                                  capture_output=True, text=True, timeout=3)
            return {"success": True, "logs": result.stdout.strip().split('\n')}
        except subprocess.TimeoutExpired:
            return {"success": False, "error": "Log reading timed out"}
        except Exception as e:
            return {"success": False, "error": f"Error reading logs: {str(e)}"}

# Convenience functions for synchronous usage
def analyze_widgets() -> Dict[str, Any]:
    """Sync wrapper for widget analysis"""
    probe = JtagAsyncProbe()
    return asyncio.run(probe.analyze_widgets_async())

def health_check() -> Dict[str, Any]:
    """Sync wrapper for health check"""
    probe = JtagAsyncProbe()
    return asyncio.run(probe.health_check_async())

def take_screenshot(selector: Optional[str] = None) -> Dict[str, Any]:
    """Sync wrapper for screenshot"""
    probe = JtagAsyncProbe()
    return asyncio.run(probe.take_screenshot_async(selector))

def execute_js(javascript_code: str) -> Dict[str, Any]:
    """Sync wrapper for JavaScript execution"""
    probe = JtagAsyncProbe()
    return asyncio.run(probe.execute_js_async(javascript_code))

# Example usage for AI debugging
if __name__ == "__main__":
    import sys
    
    if len(sys.argv) < 2:
        print("Usage: python JtagAsyncProbe.py <command> [args]")
        print("Commands: widgets, health, screenshot, js <code>")
        sys.exit(1)
    
    command = sys.argv[1]
    
    if command == "widgets":
        result = analyze_widgets()
        print(json.dumps(result, indent=2))
    elif command == "health":
        result = health_check()
        print(json.dumps(result, indent=2))
    elif command == "screenshot":
        selector = sys.argv[2] if len(sys.argv) > 2 else None
        result = take_screenshot(selector)
        print(json.dumps(result, indent=2))
    elif command == "js" and len(sys.argv) > 2:
        code = " ".join(sys.argv[2:])
        result = execute_js(code)
        print(json.dumps(result, indent=2))
    else:
        print("Unknown command")
        sys.exit(1)