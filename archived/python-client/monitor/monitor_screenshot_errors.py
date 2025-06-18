#!/usr/bin/env python3
"""
Monitor Screenshot Errors - Comprehensive Error Detection
Track all logs, server output, and file system changes during screenshot operations
"""

import asyncio
import sys
import time
import json
from pathlib import Path
from datetime import datetime

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

class ScreenshotMonitor:
    def __init__(self):
        self.screenshot_dir = Path('.continuum/screenshots')
        self.browser_logs_dir = Path('.continuum/logs/browser')
        self.start_time = time.time()
        self.initial_files = set()
        self.server_errors = []
        self.browser_errors = []
        
    def capture_initial_state(self):
        """Capture initial state of files and logs"""
        print("📊 Capturing initial state...")
        
        # Get initial screenshot files
        if self.screenshot_dir.exists():
            self.initial_files = set(f.name for f in self.screenshot_dir.glob('*.png'))
            print(f"📁 Initial screenshot files: {len(self.initial_files)}")
        else:
            print("📁 Screenshot directory doesn't exist")
            
        # Get initial browser error count
        error_file = self.browser_logs_dir / f'browser-errors-{datetime.now().strftime("%Y-%m-%d")}.log'
        if error_file.exists():
            with open(error_file) as f:
                initial_errors = len(f.readlines())
            print(f"🚨 Initial browser errors: {initial_errors}")
        else:
            print("🚨 No browser error log found")
            
    def check_new_files(self):
        """Check for new screenshot files"""
        if not self.screenshot_dir.exists():
            return []
            
        current_files = set(f.name for f in self.screenshot_dir.glob('*.png'))
        new_files = current_files - self.initial_files
        
        if new_files:
            print(f"📸 NEW FILES DETECTED: {list(new_files)}")
            for filename in new_files:
                filepath = self.screenshot_dir / filename
                size = filepath.stat().st_size
                print(f"  📄 {filename}: {size} bytes ({size//1024}KB)")
                
                # Check if file is suspiciously small (indicates failure)
                if size < 1000:  # Less than 1KB is suspicious
                    print(f"  ⚠️ WARNING: {filename} is suspiciously small ({size} bytes)")
                    
        return list(new_files)
        
    def check_browser_errors(self):
        """Check for new browser errors"""
        error_file = self.browser_logs_dir / f'browser-errors-{datetime.now().strftime("%Y-%m-%d")}.log'
        if not error_file.exists():
            return []
            
        try:
            with open(error_file) as f:
                lines = f.readlines()
                
            # Look for errors after our start time
            new_errors = []
            for line in lines:
                try:
                    error_data = json.loads(line.strip())
                    error_timestamp = datetime.fromisoformat(error_data['timestamp'].replace('Z', '+00:00')).timestamp()
                    
                    if error_timestamp > self.start_time:
                        new_errors.append(error_data)
                        print(f"🚨 NEW BROWSER ERROR: {error_data['data']['error']}")
                        
                except (json.JSONDecodeError, KeyError) as e:
                    continue
                    
            return new_errors
        except Exception as e:
            print(f"❌ Error reading browser logs: {e}")
            return []
            
    def monitor_console_output(self, js_result):
        """Monitor JavaScript console output for errors"""
        if not js_result.get('success'):
            print(f"🚨 JAVASCRIPT ERROR: {js_result.get('error', 'Unknown error')}")
            
        # Check console output for errors
        output = js_result.get('output', [])
        for entry in output:
            level = entry.get('level', '')
            message = entry.get('message', '')
            
            if level == 'error':
                print(f"🚨 CONSOLE ERROR: {message}")
            elif level == 'warn':
                print(f"⚠️ CONSOLE WARNING: {message}")
            elif 'error' in message.lower() or 'failed' in message.lower():
                print(f"⚠️ POTENTIAL ERROR IN LOG: {message}")

async def monitor_screenshot_test():
    print("🔍 COMPREHENSIVE SCREENSHOT ERROR MONITORING")
    print("=" * 60)
    
    monitor = ScreenshotMonitor()
    monitor.capture_initial_state()
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'screenshot-monitor',
            'agentName': 'Screenshot Error Monitor',
            'agentType': 'ai'
        })
        
        print("\n🧪 Testing screenshot with comprehensive monitoring...")
        
        # Test 1: Try basic screenshot command
        print("\n--- TEST 1: Basic Screenshot Command ---")
        try:
            result = await client.command.screenshot(
                selector='.version-badge',
                name_prefix='monitor_test',
                scale=1.0,
                manual=False
            )
            print(f"📊 Command result: {result}")
            
            # Monitor for errors immediately after command
            time.sleep(1)
            monitor.check_browser_errors()
            monitor.check_new_files()
            
        except Exception as e:
            print(f"🚨 EXCEPTION in screenshot command: {e}")
            
        # Test 2: Direct JavaScript execution with monitoring
        print("\n--- TEST 2: Direct JavaScript with Error Monitoring ---")
        js_result = await client.js.execute("""
            console.log('🔍 Starting monitored screenshot test');
            
            // Check html2canvas availability
            if (typeof html2canvas === 'undefined') {
                console.error('❌ html2canvas not available');
                return 'NO_HTML2CANVAS';
            }
            
            // Find target element
            const versionElement = document.querySelector('.version-badge');
            if (!versionElement) {
                console.error('❌ Version element not found');
                return 'NO_VERSION_ELEMENT';
            }
            
            console.log('✅ Version element: ' + versionElement.offsetWidth + 'x' + versionElement.offsetHeight);
            
            // Try screenshot with detailed error reporting
            try {
                console.log('📸 Starting html2canvas...');
                
                const canvas = await html2canvas(versionElement, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 1.0,
                    backgroundColor: '#1a1a1a',
                    onclone: function(clonedDoc) {
                        console.log('📋 Document cloned for html2canvas');
                    },
                    onrendered: function(canvas) {
                        console.log('🎨 Canvas rendered: ' + canvas.width + 'x' + canvas.height);
                    }
                });
                
                console.log('✅ html2canvas completed: ' + canvas.width + 'x' + canvas.height);
                
                // Create data URL
                const dataURL = canvas.toDataURL('image/png');
                console.log('✅ DataURL created, length: ' + dataURL.length);
                
                // Send via WebSocket
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    const message = {
                        type: 'screenshot_data',
                        dataURL: dataURL,
                        filename: 'monitor_direct_test.png',
                        timestamp: Date.now(),
                        dimensions: { width: canvas.width, height: canvas.height },
                        source: 'error_monitor'
                    };
                    
                    console.log('📤 Sending to WebSocket...');
                    window.ws.send(JSON.stringify(message));
                    console.log('✅ WebSocket message sent');
                    
                    return 'SUCCESS';
                } else {
                    console.error('❌ WebSocket not available');
                    return 'NO_WEBSOCKET';
                }
                
            } catch (error) {
                console.error('❌ html2canvas error:', error.name, error.message);
                console.error('❌ Error stack:', error.stack);
                return 'HTML2CANVAS_ERROR: ' + error.message;
            }
        """)
        
        print(f"📊 JavaScript result: {js_result}")
        monitor.monitor_console_output(js_result)
        
        # Wait and monitor for file creation
        print("\n🕐 Waiting 5 seconds for file creation...")
        for i in range(5):
            time.sleep(1)
            new_files = monitor.check_new_files()
            new_errors = monitor.check_browser_errors()
            
            if new_files or new_errors:
                break
                
        # Final status check
        print("\n" + "=" * 60)
        print("📊 FINAL MONITORING RESULTS:")
        
        final_files = monitor.check_new_files()
        final_errors = monitor.check_browser_errors()
        
        if final_files:
            print(f"✅ Files created: {final_files}")
        else:
            print("❌ No files created")
            
        if final_errors:
            print(f"🚨 Errors detected: {len(final_errors)}")
            for error in final_errors:
                print(f"  - {error['data']['error']}")
        else:
            print("✅ No new browser errors detected")
            
        # Save monitoring results
        results = {
            'timestamp': datetime.now().isoformat(),
            'test_duration_seconds': time.time() - monitor.start_time,
            'files_created': final_files,
            'browser_errors': final_errors,
            'js_result': js_result,
            'silent_errors_detected': len(final_errors) == 0 and len(final_files) == 0 and js_result.get('success')
        }
        
        results_file = Path('.continuum/screenshot_monitoring_results.json')
        with open(results_file, 'w') as f:
            json.dump(results, f, indent=2)
        print(f"📄 Results saved to {results_file}")
        
        return results

if __name__ == "__main__":
    results = asyncio.run(monitor_screenshot_test())
    
    if results['silent_errors_detected']:
        print("\n🚨 SILENT ERROR DETECTED: JavaScript succeeded but no files created and no browser errors")
    elif results['files_created']:
        print("\n🎉 SUCCESS: Screenshots created successfully")
    else:
        print("\n💥 FAILURE: Errors detected or no output")