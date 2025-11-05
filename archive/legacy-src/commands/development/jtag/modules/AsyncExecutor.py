#!/usr/bin/env python3

"""
JTAG Async Executor - Promise-based command execution
====================================================
Core async execution engine that safely executes JavaScript in the browser
without blocking the JTAG debugging system.
"""

import asyncio
import json
import subprocess
from typing import Dict, Any, Optional

class AsyncExecutor:
    """Promise-based command executor for JTAG system"""
    
    def __init__(self, continuum_path: str = "./continuum"):
        self.continuum_path = continuum_path
    
    async def execute_js_async(self, javascript_code: str, timeout: int = 5) -> Dict[str, Any]:
        """Execute JavaScript in browser asynchronously - won't crash main process"""
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
    
    async def execute_continuum_command_async(self, cmd: str, *args, timeout: int = 5) -> Dict[str, Any]:
        """Execute any continuum command asynchronously"""
        try:
            full_cmd = [self.continuum_path, cmd] + list(args)
            
            process = await asyncio.create_subprocess_exec(
                *full_cmd,
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
            return {"success": False, "error": f"Command timed out after {timeout}s"}
        except Exception as e:
            return {"success": False, "error": f"Command error: {str(e)}"}

# Sync wrapper for backward compatibility
def execute_js(javascript_code: str, timeout: int = 5) -> Dict[str, Any]:
    """Sync wrapper for JavaScript execution"""
    executor = AsyncExecutor()
    return asyncio.run(executor.execute_js_async(javascript_code, timeout))