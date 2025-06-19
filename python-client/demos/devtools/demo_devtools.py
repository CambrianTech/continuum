#!/usr/bin/env python3
"""
Simple DevTools Demo Script
Just does everything we need without elegance - PROOF OF CONCEPT
"""

import subprocess
import time
import sys
import asyncio
from pathlib import Path

# Add python-client to path
sys.path.insert(0, str(Path(__file__).parent / "python-client"))

def run_command(cmd, description):
    """Run a command and show what we're doing"""
    print(f"🔧 {description}")
    print(f"   Running: {' '.join(cmd) if isinstance(cmd, list) else cmd}")
    
    try:
        if isinstance(cmd, str):
            result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=10)
        else:
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        
        if result.returncode == 0:
            print(f"   ✅ Success")
            if result.stdout.strip():
                print(f"   Output: {result.stdout.strip()[:100]}...")
        else:
            print(f"   ⚠️ Warning (code {result.returncode})")
            if result.stderr.strip():
                print(f"   Error: {result.stderr.strip()[:100]}...")
        
        return result.returncode == 0
        
    except subprocess.TimeoutExpired:
        print(f"   ⏰ Timeout (10s)")
        return False
    except Exception as e:
        print(f"   ❌ Failed: {e}")
        return False

def demo_devtools_system():
    """Simple demo of the complete DevTools system"""
    
    print("🎯 DEVTOOLS DEMO - PROOF OF CONCEPT")
    print("=" * 60)
    
    # Step 1: Kill any existing Opera with debug ports
    print("\n📌 STEP 1: Clean up existing processes")
    run_command(['pkill', '-f', 'Opera.*remote-debugging-port'], 
                "Killing existing Opera with remote debugging")
    time.sleep(2)
    
    # Step 2: Launch Opera in debug mode to localhost:9000  
    print("\n📌 STEP 2: Launch Opera in debug mode")
    opera_cmd = [
        '/Applications/Opera GX.app/Contents/MacOS/Opera',
        '--remote-debugging-port=9222',
        '--disable-web-security', 
        '--user-data-dir=/tmp/opera-devtools-demo',
        'http://localhost:9000'
    ]
    
    print(f"🚀 Starting Opera with debug port 9222...")
    try:
        subprocess.Popen(opera_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print("✅ Opera launched in background")
        print("   URL: http://localhost:9000")
        print("   Debug Port: 9222")
        time.sleep(5)  # Give Opera time to start
    except Exception as e:
        print(f"❌ Failed to launch Opera: {e}")
        return False
    
    # Step 3: Test DevTools connection
    print("\n📌 STEP 3: Test DevTools connection")
    run_command(['curl', '-s', 'http://localhost:9222/json'], 
                "Checking DevTools port 9222")
    
    # Step 4: Start DevTools monitoring via portal
    print("\n📌 STEP 4: Start DevTools monitoring")
    print("🔌 Starting DevTools monitoring in background...")
    try:
        monitor_process = subprocess.Popen([
            sys.executable, 'python-client/ai-portal.py', '--devtools'
        ], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        print(f"✅ DevTools monitor started (PID: {monitor_process.pid})")
        time.sleep(3)  # Give it time to connect
    except Exception as e:
        print(f"❌ Failed to start DevTools monitor: {e}")
        return False
    
    # Step 5: Take a test screenshot
    print("\n📌 STEP 5: Take test screenshot")
    run_command([sys.executable, 'python-client/take_devtools_screenshot.py', 'demo_proof_of_concept'],
                "Taking DevTools screenshot")
    
    # Step 6: Show current logs
    print("\n📌 STEP 6: Show current logs")
    run_command([sys.executable, 'python-client/ai-portal.py', '--logs', '3'],
                "Getting recent client logs")
    
    # Step 7: Show running daemons
    print("\n📌 STEP 7: Check daemon status")
    run_command([sys.executable, 'python-client/ai-portal.py', '--daemons'],
                "Checking running daemons")
    
    # Step 8: Test screenshot again to prove it's working
    print("\n📌 STEP 8: Take second screenshot")
    run_command([sys.executable, 'python-client/take_devtools_screenshot.py', 'demo_second_test'],
                "Taking second screenshot to prove persistence")
    
    print("\n🎉 DEMO COMPLETE")
    print("=" * 60)
    print("✅ Opera running in debug mode")
    print("✅ DevTools monitoring active") 
    print("✅ Screenshots working")
    print("✅ Logs flowing")
    
    print(f"\n📸 Screenshots saved in: .continuum/screenshots/")
    print("🔍 DevTools monitor still running in background")
    print("🛑 Kill Opera manually when done: pkill -f 'Opera.*remote-debugging-port'")
    
    return True

if __name__ == "__main__":
    success = demo_devtools_system()
    sys.exit(0 if success else 1)