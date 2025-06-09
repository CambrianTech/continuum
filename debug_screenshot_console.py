#!/usr/bin/env python3
"""
Debug screenshot via browser console logs - write to myself to debug
"""
import asyncio
import websockets
import json
import base64

async def debug_screenshot_console():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Script that writes detailed debug info to console for me to read
        debug_js = """
        console.log("🔍 CLAUDE DEBUG: Starting comprehensive screenshot debug");
        
        // Step 1: Environment check
        console.log("🔍 CLAUDE DEBUG: Environment check");
        console.log("🔍 CLAUDE DEBUG: html2canvas available:", typeof html2canvas !== 'undefined');
        console.log("🔍 CLAUDE DEBUG: WebSocket available:", typeof window.ws !== 'undefined');
        console.log("🔍 CLAUDE DEBUG: WebSocket state:", window.ws ? window.ws.readyState : 'N/A');
        
        // Step 2: Canvas element analysis
        console.log("🔍 CLAUDE DEBUG: Canvas analysis");
        const canvases = document.querySelectorAll('canvas');
        console.log("🔍 CLAUDE DEBUG: Total canvas elements:", canvases.length);
        
        canvases.forEach((canvas, i) => {
            const info = {
                index: i,
                width: canvas.width,
                height: canvas.height,
                offsetWidth: canvas.offsetWidth,
                offsetHeight: canvas.offsetHeight,
                display: canvas.style.display,
                id: canvas.id || 'no-id',
                className: canvas.className || 'no-class'
            };
            console.log("🔍 CLAUDE DEBUG: Canvas " + i + ":", JSON.stringify(info));
            
            if (canvas.width === 0 || canvas.height === 0) {
                console.log("🚨 CLAUDE DEBUG: ZERO DIMENSION CANVAS DETECTED:", JSON.stringify(info));
            }
        });
        
        // Step 3: Target element selection
        console.log("🔍 CLAUDE DEBUG: Target element selection");
        let targetElement = document.querySelector('.version-badge, [class*="version"]');
        
        if (!targetElement) {
            console.log("🔍 CLAUDE DEBUG: No version badge found, creating test element");
            targetElement = document.createElement('div');
            targetElement.style.width = '100px';
            targetElement.style.height = '30px';
            targetElement.style.backgroundColor = '#0066cc';
            targetElement.style.color = 'white';
            targetElement.style.padding = '5px';
            targetElement.style.borderRadius = '5px';
            targetElement.textContent = 'TEST-v0.2.1983';
            targetElement.id = 'claude-debug-test-element';
            document.body.appendChild(targetElement);
            console.log("🔍 CLAUDE DEBUG: Created test element");
        }
        
        console.log("🔍 CLAUDE DEBUG: Target element:", targetElement.tagName, targetElement.id);
        console.log("🔍 CLAUDE DEBUG: Target dimensions:", {
            offsetWidth: targetElement.offsetWidth,
            offsetHeight: targetElement.offsetHeight,
            clientWidth: targetElement.clientWidth,
            clientHeight: targetElement.clientHeight
        });
        
        // Step 4: Canvas hiding process
        console.log("🔍 CLAUDE DEBUG: Starting canvas hiding process");
        const hiddenElements = [];
        canvases.forEach((canvas, i) => {
            if (canvas.width === 0 || canvas.height === 0) {
                console.log("🔍 CLAUDE DEBUG: Hiding canvas " + i);
                canvas.style.display = 'none';
                canvas.setAttribute('data-claude-debug-hidden', 'true');
                hiddenElements.push({canvas: canvas, index: i});
            }
        });
        console.log("🔍 CLAUDE DEBUG: Hidden " + hiddenElements.length + " problematic canvases");
        
        // Step 5: Attempt html2canvas
        console.log("🔍 CLAUDE DEBUG: Starting html2canvas attempt");
        
        if (typeof html2canvas === 'undefined') {
            console.error("🚨 CLAUDE DEBUG: html2canvas not available - ABORT");
            return "DEBUG_ABORT_NO_HTML2CANVAS";
        }
        
        try {
            const options = {
                allowTaint: true,
                useCORS: true,
                scale: 0.5,
                backgroundColor: null,
                logging: true,
                ignoreElements: function(element) {
                    const ignore = element.tagName === 'CANVAS' || 
                                 element.hasAttribute('data-claude-debug-hidden');
                    if (ignore) {
                        console.log("🔍 CLAUDE DEBUG: Ignoring element:", element.tagName, element.id);
                    }
                    return ignore;
                }
            };
            
            console.log("🔍 CLAUDE DEBUG: html2canvas options:", JSON.stringify({
                allowTaint: options.allowTaint,
                useCORS: options.useCORS,
                scale: options.scale,
                backgroundColor: options.backgroundColor,
                logging: options.logging
            }));
            
            console.log("🔍 CLAUDE DEBUG: Calling html2canvas NOW...");
            
            html2canvas(targetElement, options).then(function(canvas) {
                console.log("✅ CLAUDE DEBUG: html2canvas SUCCESS!");
                console.log("🔍 CLAUDE DEBUG: Result canvas dimensions:", canvas.width + "x" + canvas.height);
                
                // Restore hidden elements
                hiddenElements.forEach(item => {
                    console.log("🔍 CLAUDE DEBUG: Restoring canvas " + item.index);
                    item.canvas.style.display = '';
                    item.canvas.removeAttribute('data-claude-debug-hidden');
                });
                
                // Clean up test element
                const testElement = document.getElementById('claude-debug-test-element');
                if (testElement) {
                    console.log("🔍 CLAUDE DEBUG: Cleaning up test element");
                    testElement.remove();
                }
                
                // Convert to data URL
                const dataURL = canvas.toDataURL('image/png');
                console.log("🔍 CLAUDE DEBUG: DataURL length:", dataURL.length);
                console.log("🔍 CLAUDE DEBUG: DataURL preview:", dataURL.substring(0, 100) + "...");
                
                // Send via WebSocket
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    console.log("🔍 CLAUDE DEBUG: Sending screenshot data via WebSocket");
                    window.ws.send(JSON.stringify({
                        type: 'claude_debug_screenshot',
                        dataURL: dataURL,
                        width: canvas.width,
                        height: canvas.height,
                        timestamp: Date.now(),
                        success: true
                    }));
                    console.log("✅ CLAUDE DEBUG: Screenshot data sent via WebSocket");
                } else {
                    console.error("🚨 CLAUDE DEBUG: WebSocket not available for sending data");
                }
                
            }).catch(function(error) {
                console.error("🚨 CLAUDE DEBUG: html2canvas FAILED:", error.message);
                console.error("🚨 CLAUDE DEBUG: Error type:", error.constructor.name);
                console.error("🚨 CLAUDE DEBUG: Full error:", error);
                
                // Restore hidden elements even on error
                hiddenElements.forEach(item => {
                    console.log("🔍 CLAUDE DEBUG: Restoring canvas " + item.index + " after error");
                    item.canvas.style.display = '';
                    item.canvas.removeAttribute('data-claude-debug-hidden');
                });
                
                // Clean up test element
                const testElement = document.getElementById('claude-debug-test-element');
                if (testElement) {
                    console.log("🔍 CLAUDE DEBUG: Cleaning up test element after error");
                    testElement.remove();
                }
                
                // Send error via WebSocket
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    console.log("🔍 CLAUDE DEBUG: Sending error data via WebSocket");
                    window.ws.send(JSON.stringify({
                        type: 'claude_debug_screenshot_error',
                        error: error.message,
                        errorType: error.constructor.name,
                        timestamp: Date.now(),
                        success: false
                    }));
                } else {
                    console.error("🚨 CLAUDE DEBUG: WebSocket not available for sending error");
                }
            });
            
        } catch (initError) {
            console.error("🚨 CLAUDE DEBUG: Exception before html2canvas call:", initError.message);
            console.error("🚨 CLAUDE DEBUG: Init error type:", initError.constructor.name);
        }
        
        console.log("🔍 CLAUDE DEBUG: Debug script completed - check console for results");
        return "CLAUDE_DEBUG_SCRIPT_EXECUTED";
        """
        
        encoded = base64.b64encode(debug_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("🔍 Sending comprehensive debug script to browser...")
        await websocket.send(json.dumps(command))
        
        # Wait for result and read all the debug logs I wrote to myself
        for attempt in range(10):
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=2)
                data = json.loads(response)
                
                if data.get('type') == 'result':
                    # Parse console output to read my debug messages
                    try:
                        result_data = data.get('data', {})
                        inner_result = result_data.get('result', {})
                        browser_result = inner_result.get('result', {})
                        browser_response = browser_result.get('browserResponse', {})
                        console_output = browser_response.get('output', [])
                        
                        print(f"\n📋 READING MY DEBUG MESSAGES FROM BROWSER:")
                        print("=" * 60)
                        
                        for msg in console_output:
                            level = msg.get('level', 'unknown')
                            message = msg.get('message', '')
                            
                            if 'CLAUDE DEBUG' in message:
                                if level == 'error':
                                    print(f"🚨 {message}")
                                elif level == 'warn':
                                    print(f"⚠️  {message}")
                                else:
                                    print(f"📝 {message}")
                        
                        print("=" * 60)
                        break
                        
                    except Exception as e:
                        print(f"❌ Error parsing debug result: {e}")
                        break
                        
                elif data.get('type') == 'claude_debug_screenshot':
                    print(f"✅ SUCCESS: Received screenshot data!")
                    print(f"   Dimensions: {data.get('width')}x{data.get('height')}")
                    print(f"   DataURL length: {len(data.get('dataURL', ''))}")
                    
                elif data.get('type') == 'claude_debug_screenshot_error':
                    print(f"❌ ERROR: {data.get('error')}")
                    print(f"   Error type: {data.get('errorType')}")
                    
                elif data.get('type') == 'working':
                    print("⏳ Processing debug script...")
                    continue
                    
            except asyncio.TimeoutError:
                print(f"⏰ Timeout {attempt + 1}/10")
                continue
        
        return True

if __name__ == "__main__":
    asyncio.run(debug_screenshot_console())