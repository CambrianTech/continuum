#!/usr/bin/env python3
"""
Fixed Console Reader - Rebuild console reading capability
Uses the working simple test pattern to access browser console
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def read_console_properly():
    """Read browser console using working pattern"""
    print("🔧 Fixed Console Reader - Rebuilding console access")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'fixed-console-reader',
            'agentName': 'Fixed Console Reader',
            'agentType': 'ai'
        })
        
        print("📡 Connected, reading browser console...")
        
        # Use the working simple pattern (same as simple_working_test.py)
        result = await client.js.execute("""
            console.log('🔍 Fixed Console Reader starting...');
            
            // Check basic state using working pattern
            var wsStatus = window.ws ? 'Connected' : 'Disconnected';
            var wsReady = window.ws ? window.ws.readyState : 'No WebSocket';
            var screenshotUtils = typeof ScreenshotUtils !== 'undefined';
            var html2canvas = typeof html2canvas !== 'undefined';
            
            console.log('📊 Browser State:');
            console.log('  WebSocket:', wsStatus, '(readyState:', wsReady, ')');
            console.log('  ScreenshotUtils:', screenshotUtils);
            console.log('  html2canvas:', html2canvas);
            
            // Check for errors in recent console - simplified
            var errorCount = 0;
            var warningCount = 0;
            
            // Use working return pattern - no const/arrow functions
            'CONSOLE_READ_SUCCESS';
        """)
        
        print(f"✅ Console read result: {result}")
        
        if result.get('success'):
            data = eval(result.get('result', '{}'))
            print("\n📊 BROWSER CONSOLE STATE:")
            print(f"  WebSocket: {data.get('wsStatus')} (readyState: {data.get('wsReady')})")
            print(f"  ScreenshotUtils: {data.get('screenshotUtils')}")
            print(f"  html2canvas: {data.get('html2canvas')}")
            print(f"  Errors: {data.get('errorCount')}")
            print(f"  Warnings: {data.get('warningCount')}")
            
            # Show console output
            if result.get('output'):
                print("\n📝 CONSOLE OUTPUT:")
                for entry in result.get('output', []):
                    level = entry.get('level', 'log').upper()
                    message = entry.get('message', '')
                    print(f"  {level}: {message}")
        else:
            print(f"❌ Console read failed: {result}")

if __name__ == "__main__":
    asyncio.run(read_console_properly())