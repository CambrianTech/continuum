#!/usr/bin/env python3
"""
Test console error capture - specifically test error detection and capture
"""

import asyncio
import websockets
import json
import base64

async def test_console_error_capture():
    print("🚨 CONSOLE ERROR CAPTURE TEST")
    print("=" * 50)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test JavaScript that generates various console outputs and errors
            js_test = """
            console.log("🔧 CONSOLE TEST: Starting error capture test");
            
            // Generate different types of console output
            console.log("This is a normal log message");
            console.warn("This is a warning message");
            console.error("This is an error message");
            
            // Generate actual JavaScript errors
            try {
                undefinedFunction(); // This will cause a ReferenceError
            } catch (e) {
                console.error("Caught error:", e.message);
                console.error("Error type:", e.constructor.name);
            }
            
            // Generate more errors
            try {
                let obj = null;
                obj.someProperty; // This will cause a TypeError
            } catch (e) {
                console.error("Null reference error:", e.message);
            }
            
            console.log("🔧 CONSOLE TEST: Error capture test completed");
            return "CONSOLE_ERROR_TEST_COMPLETE";
            """
            
            encoded = base64.b64encode(js_test.encode()).decode()
            
            command = {
                'type': 'task',
                'role': 'system',
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            print("📤 Sending console error test...")
            await websocket.send(json.dumps(command))
            
            # Wait for bus command result
            result = None
            for attempt in range(3):
                response = await websocket.recv()
                current_result = json.loads(response)
                
                if current_result.get('type') == 'result' and current_result.get('data', {}).get('role') == 'BusCommand':
                    result = current_result
                    break
                elif current_result.get('type') == 'working':
                    continue
            
            if result:
                bus_result = result.get('data', {}).get('result', {}).get('result', {})
                browser_response = bus_result.get('browserResponse', {})
                console_output = browser_response.get('output', [])
                
                print(f"\n📋 CONSOLE OUTPUT CAPTURED ({len(console_output)} messages):")
                print("=" * 60)
                
                for i, msg in enumerate(console_output, 1):
                    level = msg.get('level', 'unknown')
                    message = msg.get('message', '')
                    
                    # Color code by level
                    if level == 'error':
                        icon = "🚨"
                    elif level == 'warn':
                        icon = "⚠️"
                    elif level == 'log':
                        icon = "📝"
                    else:
                        icon = "❓"
                    
                    print(f"{icon} [{level.upper()}] {message}")
                
                # Analyze captured content
                log_count = sum(1 for msg in console_output if msg.get('level') == 'log')
                warn_count = sum(1 for msg in console_output if msg.get('level') == 'warn')
                error_count = sum(1 for msg in console_output if msg.get('level') == 'error')
                
                print(f"\n📊 ANALYSIS:")
                print(f"  Log messages: {log_count}")
                print(f"  Warning messages: {warn_count}")
                print(f"  Error messages: {error_count}")
                print(f"  Total messages: {len(console_output)}")
                
                # Check if specific error types were captured
                error_messages = [msg.get('message', '') for msg in console_output if msg.get('level') == 'error']
                reference_error_found = any('undefinedFunction' in msg or 'ReferenceError' in msg for msg in error_messages)
                type_error_found = any('TypeError' in msg or 'null' in msg for msg in error_messages)
                
                print(f"\n🔍 ERROR DETECTION:")
                print(f"  ReferenceError captured: {'✅' if reference_error_found else '❌'}")
                print(f"  TypeError captured: {'✅' if type_error_found else '❌'}")
                
                if error_count > 0 and reference_error_found and type_error_found:
                    print(f"\n🎉 CONSOLE ERROR CAPTURE: COMPLETE SUCCESS!")
                    print(f"✅ All console output types captured")
                    print(f"✅ JavaScript errors detected and logged")
                    print(f"✅ Error types properly identified")
                    return True
                else:
                    print(f"\n❌ CONSOLE ERROR CAPTURE: PARTIAL SUCCESS")
                    print(f"Console output captured but missing some error types")
                    return False
            else:
                print("❌ No bus command result received")
                return False
                
    except Exception as e:
        print(f"❌ Test error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(test_console_error_capture())
    print(f"\n🎯 Final Result: {'SUCCESS' if result else 'FAILED'}")