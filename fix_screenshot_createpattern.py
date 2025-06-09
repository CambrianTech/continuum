#!/usr/bin/env python3
"""
Fix screenshot by completely removing ALL canvas elements before html2canvas
"""
import asyncio
import websockets
import json
import base64

async def fix_screenshot_createpattern():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Script that completely removes ALL canvas elements and tests screenshot
        fix_js = """
        console.log("🔧 SCREENSHOT FIX: Removing ALL canvas elements to prevent createPattern errors");
        
        // Step 1: Completely remove ALL canvas elements from DOM
        const allCanvases = document.querySelectorAll('canvas');
        const removedCanvases = [];
        
        allCanvases.forEach((canvas, i) => {
            console.log("🔧 REMOVING canvas " + i + ": " + canvas.width + "x" + canvas.height);
            removedCanvases.push({
                element: canvas,
                parent: canvas.parentNode,
                nextSibling: canvas.nextSibling
            });
            canvas.remove();
        });
        
        console.log("🔧 REMOVED " + removedCanvases.length + " canvas elements completely");
        
        // Step 2: Create simple test element
        const testElement = document.createElement('div');
        testElement.style.cssText = 'width:120px;height:40px;background:#0066cc;color:white;padding:8px;border-radius:6px;font-family:monospace;font-size:14px;';
        testElement.textContent = 'v0.2.1983';
        testElement.id = 'screenshot-fix-test';
        document.body.appendChild(testElement);
        
        console.log("🔧 Created clean test element for screenshot");
        
        // Step 3: Test html2canvas without ANY canvas elements
        if (typeof html2canvas === 'undefined') {
            console.error("🚨 html2canvas not available");
            return "NO_HTML2CANVAS";
        }
        
        console.log("🔧 Starting html2canvas with NO canvas elements...");
        
        return html2canvas(testElement, {
            allowTaint: true,
            useCORS: true,
            scale: 1,
            backgroundColor: null
        }).then(function(canvas) {
            console.log("✅ SCREENSHOT SUCCESS! No createPattern errors!");
            console.log("✅ Result: " + canvas.width + "x" + canvas.height);
            
            const dataURL = canvas.toDataURL('image/png');
            console.log("✅ DataURL length: " + dataURL.length);
            
            // Send success via WebSocket
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                console.log("✅ Sending screenshot success via WebSocket");
                window.ws.send(JSON.stringify({
                    type: 'validation_screenshot',
                    dataURL: dataURL,
                    width: canvas.width,
                    height: canvas.height,
                    timestamp: Date.now(),
                    success: true
                }));
                console.log("✅ Screenshot data sent successfully");
            }
            
            // Restore canvas elements
            console.log("🔧 Restoring " + removedCanvases.length + " canvas elements");
            removedCanvases.forEach((item, i) => {
                if (item.parent) {
                    if (item.nextSibling) {
                        item.parent.insertBefore(item.element, item.nextSibling);
                    } else {
                        item.parent.appendChild(item.element);
                    }
                }
                console.log("🔧 Restored canvas " + i);
            });
            
            // Clean up test element
            testElement.remove();
            console.log("🔧 Cleanup complete");
            
            return {
                success: true,
                width: canvas.width,
                height: canvas.height,
                dataLength: dataURL.length
            };
            
        }).catch(function(error) {
            console.error("❌ SCREENSHOT STILL FAILED: " + error.message);
            console.error("❌ Error type: " + error.constructor.name);
            
            // Send error via WebSocket
            if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                window.ws.send(JSON.stringify({
                    type: 'validation_screenshot_error',
                    error: error.message,
                    timestamp: Date.now(),
                    success: false
                }));
            }
            
            // Restore canvas elements even on error
            removedCanvases.forEach((item, i) => {
                if (item.parent) {
                    if (item.nextSibling) {
                        item.parent.insertBefore(item.element, item.nextSibling);
                    } else {
                        item.parent.appendChild(item.element);
                    }
                }
            });
            
            // Clean up test element
            testElement.remove();
            
            throw error;
        });
        """
        
        encoded = base64.b64encode(fix_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("🔧 Sending screenshot fix - completely removing all canvas elements...")
        await websocket.send(json.dumps(command))
        
        # Wait for results
        for attempt in range(10):
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=2)
                data = json.loads(response)
                
                if data.get('type') == 'validation_screenshot':
                    print(f"✅ SCREENSHOT SUCCESS! {data.get('width')}x{data.get('height')}")
                    print(f"   DataURL length: {len(data.get('dataURL', ''))}")
                    return True
                    
                elif data.get('type') == 'validation_screenshot_error':
                    print(f"❌ Screenshot still failed: {data.get('error')}")
                    return False
                    
                elif data.get('type') == 'result':
                    print("📤 Command executed, waiting for screenshot data...")
                    continue
                    
                elif data.get('type') == 'working':
                    continue
                    
            except asyncio.TimeoutError:
                print(f"⏰ Waiting... {attempt + 1}/10")
                continue
        
        print("⏰ Timeout - no screenshot result received")
        return False

if __name__ == "__main__":
    result = asyncio.run(fix_screenshot_createpattern())
    print(f"\n🎯 SCREENSHOT FIX: {'SUCCESS' if result else 'FAILED'}")