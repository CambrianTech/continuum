#!/usr/bin/env python3
"""
Simple canvas debug
"""
import asyncio
import websockets
import json
import base64

async def simple_canvas_debug():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Simple debug script
        debug_js = """
        console.log("=== CANVAS DEBUG ===");
        const canvases = document.querySelectorAll('canvas');
        console.log("Total canvas elements:", canvases.length);
        
        canvases.forEach((canvas, i) => {
            console.log(`Canvas ${i}:`, {
                width: canvas.width,
                height: canvas.height,
                offsetWidth: canvas.offsetWidth,
                offsetHeight: canvas.offsetHeight,
                id: canvas.id,
                className: canvas.className
            });
            
            if (canvas.width === 0 || canvas.height === 0) {
                console.error(`🚨 ZERO DIMENSION CANVAS ${i}:`, canvas);
            }
        });
        
        return "DEBUG_COMPLETE";
        """
        
        encoded = base64.b64encode(debug_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system', 
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        await websocket.send(json.dumps(command))
        
        # Just get the working response
        response = await websocket.recv()
        print("Debug sent - check browser console for output")
        
        return True

if __name__ == "__main__":
    asyncio.run(simple_canvas_debug())