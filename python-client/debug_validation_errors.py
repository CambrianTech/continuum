#!/usr/bin/env python3
"""
Debug validation errors by examining console output and server responses
"""

import asyncio
import websockets
import json
import base64

async def debug_validation_errors():
    print("🔍 DEBUGGING VALIDATION ERRORS")
    print("=" * 50)
    
    uri = "ws://localhost:9000"
    
    try:
        async with websockets.connect(uri) as websocket:
            print("✅ Connected to Continuum")
            
            # Skip initial messages
            await websocket.recv()
            await websocket.recv()
            
            # Test 1: Check what errors exist in browser console right now
            print("\n🔍 Step 1: Check existing browser console errors")
            
            console_check = """
            console.log("🔍 VALIDATION DEBUG: Checking for existing errors");
            
            // Check for any existing errors in the page
            const errors = [];
            
            // Override console methods to capture any new errors
            const originalError = console.error;
            window.validationErrors = window.validationErrors || [];
            
            console.error = function(...args) {
                window.validationErrors.push({
                    timestamp: Date.now(),
                    message: args.join(' '),
                    stack: new Error().stack
                });
                originalError.apply(console, args);
            };
            
            // Check for JavaScript errors in the page
            let hasErrors = window.validationErrors.length > 0;
            let errorCount = window.validationErrors.length;
            
            console.log("🔍 Existing errors found:", errorCount);
            
            if (window.validationErrors.length > 0) {
                console.log("🔍 Recent errors:");
                window.validationErrors.forEach((err, i) => {
                    console.log(`  ${i+1}. ${err.message}`);
                });
            }
            
            // Test WebSocket connection status
            console.log("🔍 WebSocket status:", {
                exists: !!window.ws,
                readyState: window.ws ? window.ws.readyState : 'N/A',
                url: window.ws ? window.ws.url : 'N/A'
            });
            
            // Test if validation components are working
            console.log("🔍 Page validation check:");
            console.log("  Document ready:", document.readyState);
            console.log("  HTML2Canvas available:", typeof html2canvas !== 'undefined');
            console.log("  WebSocket available:", typeof WebSocket !== 'undefined');
            
            return {
                existingErrors: errorCount,
                hasWebSocket: !!window.ws,
                wsReadyState: window.ws ? window.ws.readyState : null,
                pageReady: document.readyState === 'complete',
                html2canvasAvailable: typeof html2canvas !== 'undefined'
            };
            """
            
            encoded = base64.b64encode(console_check.encode()).decode()
            command = {
                'type': 'task',
                'role': 'system', 
                'task': f'[CMD:BROWSER_JS] {encoded}'
            }
            
            await websocket.send(json.dumps(command))
            
            # Get result
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
                return_value = browser_response.get('result')
                
                print(f"\n📋 BROWSER VALIDATION STATUS:")
                if return_value:
                    try:
                        status = json.loads(return_value) if isinstance(return_value, str) else return_value
                        print(f"  Existing errors: {status.get('existingErrors', 'unknown')}")
                        print(f"  WebSocket exists: {status.get('hasWebSocket', 'unknown')}")
                        print(f"  WebSocket state: {status.get('wsReadyState', 'unknown')}")
                        print(f"  Page ready: {status.get('pageReady', 'unknown')}")
                        print(f"  html2canvas available: {status.get('html2canvasAvailable', 'unknown')}")
                    except:
                        print(f"  Raw return value: {return_value}")
                
                print(f"\n📋 CONSOLE MESSAGES ({len(console_output)}):")
                for msg in console_output:
                    level = msg.get('level', 'unknown')
                    message = msg.get('message', '')
                    if level == 'error':
                        print(f"🚨 ERROR: {message}")
                    elif level == 'warn':
                        print(f"⚠️  WARN: {message}")
                    else:
                        print(f"📝 {level.upper()}: {message}")
                
                # Test 2: Try a simple validation command that should work
                print(f"\n🔍 Step 2: Test simple validation that should work")
                
                simple_test = """
                console.log("🔍 SIMPLE VALIDATION: Testing basic functionality");
                
                try {
                    // Test basic JavaScript execution
                    const testResult = {
                        basicMath: 2 + 2,
                        stringOp: "hello" + " world",
                        arrayOp: [1,2,3].length,
                        timestamp: Date.now()
                    };
                    
                    console.log("🔍 Basic operations successful:", testResult);
                    
                    return {
                        success: true,
                        testResult: testResult,
                        message: "Simple validation passed"
                    };
                    
                } catch (error) {
                    console.error("🔍 Simple validation failed:", error);
                    return {
                        success: false,
                        error: error.message
                    };
                }
                """
                
                encoded_simple = base64.b64encode(simple_test.encode()).decode()
                simple_command = {
                    'type': 'task',
                    'role': 'system',
                    'task': f'[CMD:BROWSER_JS] {encoded_simple}'
                }
                
                await websocket.send(json.dumps(simple_command))
                
                # Get simple test result
                simple_result = None
                for attempt in range(3):
                    response = await websocket.recv()
                    current_result = json.loads(response)
                    
                    if current_result.get('type') == 'result' and current_result.get('data', {}).get('role') == 'BusCommand':
                        simple_result = current_result
                        break
                    elif current_result.get('type') == 'working':
                        continue
                
                if simple_result:
                    simple_bus_result = simple_result.get('data', {}).get('result', {}).get('result', {})
                    simple_browser_response = simple_bus_result.get('browserResponse', {})
                    simple_return = simple_browser_response.get('result')
                    
                    print(f"\n📋 SIMPLE VALIDATION RESULT:")
                    if simple_return:
                        try:
                            simple_status = json.loads(simple_return) if isinstance(simple_return, str) else simple_return
                            print(f"  Success: {simple_status.get('success')}")
                            print(f"  Message: {simple_status.get('message')}")
                            if simple_status.get('testResult'):
                                print(f"  Test results: {simple_status.get('testResult')}")
                        except:
                            print(f"  Raw result: {simple_return}")
                    
                    simple_console = simple_browser_response.get('output', [])
                    print(f"\n📋 SIMPLE TEST CONSOLE ({len(simple_console)} messages):")
                    for msg in simple_console:
                        level = msg.get('level', 'unknown')
                        message = msg.get('message', '')
                        if level == 'error':
                            print(f"🚨 ERROR: {message}")
                        elif level == 'warn':
                            print(f"⚠️  WARN: {message}")
                        else:
                            print(f"📝 {level.upper()}: {message}")
                
                return True
            else:
                print("❌ No result received from browser console check")
                return False
                
    except Exception as e:
        print(f"❌ Debug error: {e}")
        return False

if __name__ == "__main__":
    result = asyncio.run(debug_validation_errors())
    print(f"\n🎯 Debug completed: {'SUCCESS' if result else 'FAILED'}")