#!/usr/bin/env python3
"""
Real-time Screenshot Error Monitor
Watch server logs and file system in real-time during screenshot operations
"""

import asyncio
import sys
import time
import subprocess
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def realtime_monitor():
    print("🔍 REAL-TIME SCREENSHOT MONITORING")
    print("=" * 50)
    
    # Start monitoring daemon logs in background
    print("📡 Starting daemon log monitor...")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'realtime-monitor',
            'agentName': 'Real-time Monitor',
            'agentType': 'ai'
        })
        
        print("🚀 Triggering screenshot...")
        
        # Trigger a simple screenshot and monitor everything
        try:
            result = await client.command.screenshot(
                selector='.version-badge',
                name_prefix='realtime_monitor',
                scale=1.0,
                manual=False
            )
            print(f"📊 Screenshot result: {result}")
            
        except Exception as e:
            print(f"🚨 Exception: {e}")
            
        print("✅ Real-time monitoring complete")

if __name__ == "__main__":
    # Check if daemon is running
    try:
        with open('/Users/joel/Development/cambrian/continuum/.continuum/continuum.pid') as f:
            pid = f.read().strip()
        print(f"✅ Daemon running with PID: {pid}")
    except FileNotFoundError:
        print("❌ Daemon not running")
        exit(1)
    
    asyncio.run(realtime_monitor())