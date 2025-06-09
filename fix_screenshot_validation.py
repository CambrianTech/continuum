#!/usr/bin/env python3
"""
Fix screenshot by completely removing all canvas elements
"""
import asyncio
import websockets
import json
import base64

async def fix_screenshot_validation():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Script to remove ALL canvas elements and test screenshot
        fix_js = """
        console.log("🔧 FIXING SCREENSHOT BY REMOVING ALL CANVAS ELEMENTS");
        
        // Remove ALL canvas elements from the page
        const canvases = document.querySelectorAll('canvas');
        console.log("Removing", canvases.length, "canvas elements");
        
        const removed = [];
        canvases.forEach((canvas, i) => {
            console.log(`Removing canvas ${i}:`, {
                width: canvas.width,
                height: canvas.height,
                id: canvas.id || 'no-id',
                className: canvas.className || 'no-class'
            });
            removed.push({
                element: canvas,
                parent: canvas.parentNode
            });
            canvas.remove();
        });
        
        console.log("All canvas elements removed. Testing screenshot...");
        
        // Now test html2canvas without ANY canvas elements
        if (typeof html2canvas !== 'undefined') {
            html2canvas(document.body, {
                allowTaint: true,
                useCORS: true,
                scale: 0.2
            }).then(function(canvas) {
                console.log("✅ SCREENSHOT SUCCESS! No createPattern errors!");
                console.log("Canvas size:", canvas.width + "x" + canvas.height);
                
                // Restore canvas elements
                removed.forEach(item => {
                    if (item.parent) {
                        item.parent.appendChild(item.element);
                    }
                });
                console.log("Canvas elements restored");
                
                return {
                    success: true,
                    canvasSize: canvas.width + "x" + canvas.height,
                    removedCount: removed.length
                };
                
            }).catch(function(error) {
                console.error("❌ SCREENSHOT STILL FAILED:", error.message);
                
                // Restore canvas elements even on failure
                removed.forEach(item => {
                    if (item.parent) {
                        item.parent.appendChild(item.element);
                    }
                });
                console.log("Canvas elements restored after failure");
                
                return {
                    success: false,
                    error: error.message,
                    removedCount: removed.length
                };
            });
        } else {
            console.error("html2canvas not available");
            return { success: false, error: "html2canvas_missing" };
        }
        
        return "SCREENSHOT_FIX_INITIATED";
        """
        
        encoded = base64.b64encode(fix_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("🔧 Sending screenshot fix - removing all canvas elements...")
        await websocket.send(json.dumps(command))
        
        response = await websocket.recv()
        print("Fix sent - check browser console to see if screenshot works without canvas elements")
        
        return True

if __name__ == "__main__":
    asyncio.run(fix_screenshot_validation())