#!/usr/bin/env python3
"""
Simple debug logger - write debug info to console for me to read later
"""
import asyncio
import websockets
import json
import base64

async def write_debug_logs():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Simple debug script that just writes comprehensive logs
        debug_js = """
        // CLAUDE DEBUG SESSION - Writing comprehensive debug info
        console.log("=== CLAUDE DEBUG SESSION START ===");
        
        // Environment check
        console.log("CLAUDE_DEBUG_ENV: html2canvas =", typeof html2canvas !== 'undefined');
        console.log("CLAUDE_DEBUG_ENV: WebSocket =", typeof window.ws !== 'undefined');
        if (window.ws) {
            console.log("CLAUDE_DEBUG_ENV: WS readyState =", window.ws.readyState);
        }
        
        // Canvas analysis
        const canvases = document.querySelectorAll('canvas');
        console.log("CLAUDE_DEBUG_CANVAS: Total count =", canvases.length);
        
        let zeroCanvasCount = 0;
        canvases.forEach((canvas, i) => {
            const isZero = canvas.width === 0 || canvas.height === 0;
            if (isZero) zeroCanvasCount++;
            
            console.log("CLAUDE_DEBUG_CANVAS_" + i + ": w=" + canvas.width + " h=" + canvas.height + 
                       " ow=" + canvas.offsetWidth + " oh=" + canvas.offsetHeight + 
                       " id=" + (canvas.id || 'none') + " zero=" + isZero);
        });
        
        console.log("CLAUDE_DEBUG_CANVAS: Zero dimension count =", zeroCanvasCount);
        
        // Test target element
        let target = document.querySelector('.version-badge, [class*="version"]');
        if (!target) {
            target = document.createElement('div');
            target.style.cssText = 'width:100px;height:30px;background:#0066cc;color:white;padding:5px;';
            target.textContent = 'DEBUG-TEST';
            target.id = 'debug-target';
            document.body.appendChild(target);
            console.log("CLAUDE_DEBUG_TARGET: Created test element");
        }
        
        console.log("CLAUDE_DEBUG_TARGET: Element =", target.tagName, target.id || 'no-id');
        console.log("CLAUDE_DEBUG_TARGET: Dimensions =", target.offsetWidth + "x" + target.offsetHeight);
        
        // Try html2canvas with detailed logging
        if (typeof html2canvas !== 'undefined') {
            console.log("CLAUDE_DEBUG_SCREENSHOT: Starting html2canvas...");
            
            html2canvas(target, {
                allowTaint: true,
                useCORS: true,
                scale: 0.5,
                logging: true
            }).then(function(canvas) {
                console.log("CLAUDE_DEBUG_SUCCESS: Screenshot captured!");
                console.log("CLAUDE_DEBUG_SUCCESS: Canvas size =", canvas.width + "x" + canvas.height);
                
                const dataURL = canvas.toDataURL('image/png');
                console.log("CLAUDE_DEBUG_SUCCESS: DataURL length =", dataURL.length);
                
                // Send via WebSocket
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    window.ws.send(JSON.stringify({
                        type: 'debug_screenshot_success',
                        width: canvas.width,
                        height: canvas.height,
                        dataLength: dataURL.length
                    }));
                    console.log("CLAUDE_DEBUG_SUCCESS: Data sent via WebSocket");
                }
                
                // Cleanup
                const testEl = document.getElementById('debug-target');
                if (testEl) testEl.remove();
                
            }).catch(function(error) {
                console.log("CLAUDE_DEBUG_ERROR: html2canvas failed!");
                console.log("CLAUDE_DEBUG_ERROR: Message =", error.message);
                console.log("CLAUDE_DEBUG_ERROR: Type =", error.constructor.name);
                
                if (error.message.includes('createPattern')) {
                    console.log("CLAUDE_DEBUG_ERROR: CONFIRMED createPattern error - zero canvas elements detected");
                }
                
                // Send error via WebSocket
                if (window.ws && window.ws.readyState === WebSocket.OPEN) {
                    window.ws.send(JSON.stringify({
                        type: 'debug_screenshot_error',
                        error: error.message,
                        errorType: error.constructor.name
                    }));
                }
                
                // Cleanup
                const testEl = document.getElementById('debug-target');
                if (testEl) testEl.remove();
            });
        } else {
            console.log("CLAUDE_DEBUG_ERROR: html2canvas not available");
        }
        
        console.log("=== CLAUDE DEBUG SESSION END ===");
        return "DEBUG_LOGS_WRITTEN";
        """
        
        encoded = base64.b64encode(debug_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("📝 Writing debug logs to browser console...")
        await websocket.send(json.dumps(command))
        
        response = await websocket.recv()
        print("✅ Debug logs written - now run validation to read them")
        
        return True

if __name__ == "__main__":
    asyncio.run(write_debug_logs())