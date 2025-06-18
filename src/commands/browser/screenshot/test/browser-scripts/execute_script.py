#!/usr/bin/env python3
"""
Browser Script Executor
========================

Executes JavaScript files in the browser via bus commands.
"""

import asyncio
import click
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.append(str(Path(__file__).parent.parent))

from claude_debugger.connection import WebSocketConnection
from claude_debugger.validation import JavaScriptValidator
from continuum_client.utils import get_continuum_ws_url, load_continuum_config


async def execute_browser_script(script_path):
    """Execute a JavaScript file in the browser"""
    
    if not Path(script_path).exists():
        print(f"❌ Script not found: {script_path}")
        return
    
    # Load and prepare script
    with open(script_path, 'r') as f:
        js_code = f.read()
    
    # Connect to browser
    load_continuum_config()
    ws_url = get_continuum_ws_url()
    connection = WebSocketConnection(ws_url)
    await connection.connect()
    
    # Skip banner messages
    try:
        await connection.receive_message(timeout=1)
        await connection.receive_message(timeout=1)
    except:
        pass
    
    js_validator = JavaScriptValidator(connection)
    
    print(f"🔧 Executing {Path(script_path).name} in browser...")
    
    # Execute script
    result = await js_validator.execute_and_wait(js_code)
    
    if result:
        print(f"✅ Script execution successful")
        print(f"🎯 Result: {result}")
    else:
        print(f"❌ Script execution failed")
    
    await connection.disconnect()
    return result


@click.command()
@click.argument('script_file', type=click.Path(exists=True))
async def main(script_file):
    """Execute a JavaScript file in the browser client via bus commands"""
    await execute_browser_script(script_file)


if __name__ == "__main__":
    asyncio.run(main.main(standalone_mode=False))