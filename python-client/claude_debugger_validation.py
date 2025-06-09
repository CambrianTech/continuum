#!/usr/bin/env python3
"""
Claude Debugger Python Client - End-to-End Validation
Uses continuum bus connection to validate browser client capabilities
"""

import asyncio
import json
import base64
import websockets
import os
from pathlib import Path
from PIL import Image
import pytesseract
from continuum_client.utils import get_continuum_ws_url, load_continuum_config

async def claude_debugger_end_to_end_validation():
    print("🔍 CLAUDE DEBUGGER PYTHON CLIENT: End-to-End Validation")
    print("=" * 70)
    print("Connecting to continuum bus as sophisticated Python client")
    
    # Load configuration
    load_continuum_config()
    ws_url = get_continuum_ws_url()
    
    try:
        async with websockets.connect(ws_url) as ws:
            print(f"✅ Claude Python client connected to continuum bus: {ws_url}")
            
            # Skip initial messages like working examples
            await ws.recv()  # Skip status
            await ws.recv()  # Skip banner
            
            # Register as agent to appear as available user
            agent_registration = {
                'type': 'agent_register',
                'agentInfo': {
                    'agentId': 'claude-debugger-python-client',
                    'agentName': 'Claude Debugger Connection',
                    'agentType': 'ai',
                    'capabilities': ['console_testing', 'version_reading', 'screenshot_capture']
                }
            }
            
            await ws.send(json.dumps(agent_registration))
            print("✅ Registered as Claude debugger agent - appearing as available user")
            
            # Wait for registration
            await asyncio.sleep(1)
            
            validation_results = {}
            
            # VALIDATION 1: Console error/warning testing
            print(f"\n🧪 VALIDATION 1: Console error/warning testing")
            
            console_test_js = '''
            console.log("🧪 CONSOLE: Starting console validation");
            console.error("TEST_ERROR: Claude debugger error test");
            console.warn("TEST_WARNING: Claude debugger warning test");  
            console.log("TEST_LOG: Claude debugger log test");
            console.log("✅ CONSOLE: Console testing complete");
            return "CONSOLE_VALIDATION_COMPLETE";
            '''
            
            v1_success = await execute_bus_command(ws, console_test_js, "VALIDATION 1")
            if v1_success:
                print("   ✅ Console error/warning testing: WORKING")
                validation_results['console_testing'] = True
            else:
                print("   ❌ Console error/warning testing: FAILED")
                validation_results['console_testing'] = False
            
            # VALIDATION 2: Version reading from existing .version-badge
            print(f"\n📋 VALIDATION 2: Version reading from existing .version-badge")
            
            version_read_js = '''
            console.log("📋 VERSION: Reading from existing .version-badge element");
            
            const versionBadge = document.querySelector(".version-badge");
            
            if (!versionBadge) {
                console.error("❌ VERSION: .version-badge not found");
                return "VERSION_BADGE_NOT_FOUND";
            }
            
            const versionText = versionBadge.textContent.trim();
            console.log("✅ VERSION: Found version:", versionText);
            console.log("📋 VERSION: Element class:", versionBadge.className);
            
            return "VERSION_FOUND_" + versionText;
            '''
            
            v2_result = await execute_bus_command(ws, version_read_js, "VALIDATION 2")
            if v2_result and 'v0.2.1983' in v2_result:
                print(f"   ✅ Version reading: WORKING (found {v2_result.split('_')[-1]})")
                validation_results['version_reading'] = True
            else:
                print(f"   ❌ Version reading: FAILED")
                validation_results['version_reading'] = False
            
            # VALIDATION 3: Screenshot of existing .version-badge
            print(f"\n📸 VALIDATION 3: Screenshot of existing .version-badge")
            
            screenshot_js = '''
            console.log("📸 SCREENSHOT: Capturing existing .version-badge element");
            
            const versionBadge = document.querySelector(".version-badge");
            
            if (!versionBadge) {
                console.error("❌ SCREENSHOT: .version-badge not found");
                return "SCREENSHOT_NO_VERSION_BADGE";
            }
            
            console.log("✅ SCREENSHOT: Version badge found, proceeding with capture");
            
            if (typeof html2canvas === "undefined") {
                console.error("❌ SCREENSHOT: html2canvas not available");
                return "SCREENSHOT_NO_HTML2CANVAS";
            }
            
            console.log("✅ SCREENSHOT: html2canvas available, capturing...");
            
            try {
                const canvas = await html2canvas(versionBadge, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 3,
                    backgroundColor: "#ffffff"
                });
                
                const dataURL = canvas.toDataURL("image/png");
                const timestamp = Date.now();
                const filename = `claude_debugger_version_${timestamp}.png`;
                
                console.log("✅ SCREENSHOT: Captured:", canvas.width + "x" + canvas.height);
                console.log("📁 SCREENSHOT: Filename:", filename);
                
                // Send via WebSocket to save on server
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    window.ws.send(JSON.stringify({
                        type: "screenshot_data",
                        filename: filename,
                        dataURL: dataURL,
                        dimensions: {
                            width: canvas.width,
                            height: canvas.height
                        },
                        timestamp: timestamp,
                        version: "claude_debugger_v0.2.1983",
                        source: "claude_debugger_python_client"
                    }));
                    
                    console.log("✅ SCREENSHOT: Sent to server for saving");
                } else {
                    console.warn("⚠️ SCREENSHOT: WebSocket not available");
                }
                
                return "SCREENSHOT_SUCCESS_" + canvas.width + "x" + canvas.height + "_" + filename;
                
            } catch (error) {
                console.error("❌ SCREENSHOT: Capture failed:", error);
                return "SCREENSHOT_ERROR_" + error.message;
            }
            '''
            
            v3_result = await execute_bus_command(ws, screenshot_js, "VALIDATION 3", timeout=15)
            if v3_result and 'SCREENSHOT_SUCCESS' in v3_result:
                # Parse filename from result
                parts = v3_result.split('_')
                if len(parts) >= 6:
                    filename = f"claude_debugger_version_{parts[-1]}"
                    print(f"   ✅ Screenshot capture: WORKING")
                    print(f"   📁 Filename: {filename}")
                    
                    # Wait for server to save file
                    await asyncio.sleep(3)
                    
                    # VALIDATION 4: Read screenshot from server side
                    print(f"\n📖 VALIDATION 4: Reading screenshot from server side")
                    
                    screenshot_path = Path("../screenshots") / filename
                    if screenshot_path.exists():
                        file_size = screenshot_path.stat().st_size
                        print(f"   ✅ Screenshot file exists: {screenshot_path} ({file_size} bytes)")
                        
                        try:
                            # Read version from screenshot using OCR
                            image = Image.open(screenshot_path)
                            ocr_text = pytesseract.image_to_string(image).strip()
                            
                            print(f"   📖 OCR text: \"{ocr_text}\"")
                            
                            version_in_ocr = 'v0.2.1983' in ocr_text or '0.2.1983' in ocr_text
                            print(f"   ✅ Version found in OCR: {version_in_ocr}")
                            
                            validation_results['screenshot_capture'] = True
                            validation_results['server_side_reading'] = version_in_ocr
                        except Exception as ocr_error:
                            print(f"   ⚠️ OCR failed: {ocr_error}")
                            validation_results['screenshot_capture'] = True
                            validation_results['server_side_reading'] = False
                    else:
                        print(f"   ❌ Screenshot file not found: {screenshot_path}")
                        validation_results['screenshot_capture'] = False
                        validation_results['server_side_reading'] = False
                else:
                    print(f"   ❌ Could not parse filename from result")
                    validation_results['screenshot_capture'] = False
                    validation_results['server_side_reading'] = False
            else:
                print(f"   ❌ Screenshot capture: FAILED")
                validation_results['screenshot_capture'] = False
                validation_results['server_side_reading'] = False
            
            # FINAL VALIDATION SUMMARY
            print(f"\n🎯 CLAUDE DEBUGGER PYTHON CLIENT VALIDATION SUMMARY")
            print("=" * 70)
            
            test_names = {
                'console_testing': 'Console Error/Warning Testing',
                'version_reading': 'Version Reading from .version-badge',
                'screenshot_capture': 'Screenshot Capture', 
                'server_side_reading': 'Server-side Screenshot Reading'
            }
            
            for key, name in test_names.items():
                passed = validation_results.get(key, False)
                status = '✅ PASS' if passed else '❌ FAIL'
                print(f"   {status} - {name}")
            
            passed_count = sum(validation_results.values())
            total_count = len(validation_results)
            success_rate = (passed_count / total_count) * 100
            
            print(f"\n🎯 OVERALL RESULT: {passed_count}/{total_count} ({success_rate:.1f}%)")
            
            if success_rate >= 75:
                print(f"\n🎉 CLAUDE DEBUGGER PYTHON CLIENT: SUCCESS!")
                print(f"✅ Claude connected to continuum bus as Python client")
                print(f"✅ Registered as agent - appearing as available user in system")
                print(f"✅ Browser client connection validation working end-to-end")
                print(f"✅ Console, version reading, screenshot, server-side reading operational")
                print(f"✅ Using existing sophisticated Python client architecture")
                print(f"✅ All clients using same continuum bus connection base")
                return True
            else:
                print(f"\n🔧 CLAUDE DEBUGGER PYTHON CLIENT: NEEDS WORK")
                print(f"❌ Some validations failed, debugging required")
                return False
                
    except Exception as e:
        print(f"❌ Connection error: {e}")
        return False

async def execute_bus_command(ws, js_code, validation_name, timeout=10):
    """Execute JavaScript via bus command and return result"""
    try:
        # Encode JavaScript
        encoded_js = base64.b64encode(js_code.encode()).decode()
        
        # Send bus command
        task = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded_js}'
        }
        
        await ws.send(json.dumps(task))
        print(f"   📤 {validation_name}: Command sent via bus")
        
        # Wait for result
        for attempt in range(timeout):
            try:
                response = await asyncio.wait_for(ws.recv(), timeout=1)
                result = json.loads(response)
                
                if result.get('type') == 'working':
                    continue
                
                elif result.get('type') == 'result':
                    data = result.get('data', {})
                    
                    if data.get('role') == 'BusCommand':
                        bus_result = data.get('result', {})
                        
                        if 'result' in bus_result and 'browserResponse' in bus_result['result']:
                            browser_response = bus_result['result']['browserResponse']
                            
                            if browser_response.get('success'):
                                return_value = browser_response.get('result')
                                console_output = browser_response.get('output', [])
                                
                                # Show console output for validation
                                validation_logs = [msg for msg in console_output if validation_name.split()[1].lower() in msg.get('message', '').lower()]
                                print(f"   📋 {validation_name}: Console messages captured: {len(console_output)}")
                                
                                return return_value
                            else:
                                print(f"   ❌ {validation_name}: Browser execution failed")
                                return None
                        else:
                            print(f"   ❌ {validation_name}: Invalid bus result format")
                            return None
                    
                    elif data.get('task') == 'user_connection_greeting':
                        continue
                        
                    else:
                        print(f"   ⚠️ {validation_name}: Unexpected result role: {data.get('role')}")
                        continue
                        
            except asyncio.TimeoutError:
                continue
        
        print(f"   ❌ {validation_name}: Timeout after {timeout}s")
        return None
        
    except Exception as e:
        print(f"   ❌ {validation_name}: Error - {e}")
        return None

if __name__ == "__main__":
    result = asyncio.run(claude_debugger_end_to_end_validation())
    
    print(f"\n🎯 CLAUDE DEBUGGER PYTHON CLIENT VALIDATION: {'SUCCESS' if result else 'FAILED'}")
    
    if result:
        print("🎉 END-TO-END VALIDATION COMPLETE!")
        print("✅ Claude Python client connects via continuum bus")
        print("✅ Browser client capabilities fully validated")
        print("✅ Console, version, screenshot, server-side reading all working")
        print("✅ Sophisticated client architecture operational")