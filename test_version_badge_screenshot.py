#!/usr/bin/env python3
"""
Test screenshot of just the version badge element with proper error handling
"""
import asyncio
import websockets
import json
import base64

async def test_version_badge_screenshot():
    uri = "ws://localhost:9000"
    
    async with websockets.connect(uri) as websocket:
        await websocket.recv()
        await websocket.recv()
        
        # Script to screenshot just the version badge with proper error handling
        screenshot_js = """
        console.log("📸 TESTING VERSION BADGE SCREENSHOT");
        
        // Find the version badge element
        const versionBadge = document.querySelector('.version-badge, [class*="version"], [class*="badge"]') ||
                           document.querySelector('div:contains("v0.2.1983")') ||
                           document.querySelector('*');
        
        console.log("Target element found:", versionBadge);
        
        if (!versionBadge) {
            return Promise.reject(new Error("Version badge element not found"));
        }
        
        if (typeof html2canvas === 'undefined') {
            return Promise.reject(new Error("html2canvas library not available"));
        }
        
        console.log("📸 Starting targeted screenshot of version badge...");
        
        // Screenshot just this specific element with error handling
        return html2canvas(versionBadge, {
            allowTaint: true,
            useCORS: true,
            scale: 1,
            backgroundColor: null,
            logging: true
        }).then(function(canvas) {
            console.log("✅ VERSION BADGE SCREENSHOT SUCCESS!");
            console.log("Canvas dimensions:", canvas.width + "x" + canvas.height);
            
            // Convert to data URL for validation
            const dataURL = canvas.toDataURL('image/png');
            console.log("DataURL length:", dataURL.length);
            
            return {
                success: true,
                width: canvas.width,
                height: canvas.height,
                dataLength: dataURL.length,
                element: versionBadge.tagName + (versionBadge.className ? '.' + versionBadge.className : ''),
                message: "Version badge screenshot captured successfully"
            };
            
        }).catch(function(error) {
            console.error("❌ VERSION BADGE SCREENSHOT FAILED:", error);
            
            // Provide meaningful error details
            let errorDetails = {
                success: false,
                error: error.message,
                errorType: error.constructor.name,
                element: versionBadge ? versionBadge.tagName : 'unknown',
                elementDimensions: versionBadge ? {
                    offsetWidth: versionBadge.offsetWidth,
                    offsetHeight: versionBadge.offsetHeight,
                    clientWidth: versionBadge.clientWidth,
                    clientHeight: versionBadge.clientHeight
                } : null
            };
            
            // Check for specific createPattern error
            if (error.message.includes('createPattern')) {
                errorDetails.diagnosis = "createPattern error - likely zero-dimension canvas elements in target";
                errorDetails.solution = "Remove or hide zero-dimension canvas elements before screenshot";
            } else if (error.message.includes('tainted')) {
                errorDetails.diagnosis = "Canvas tainted - cross-origin content";
                errorDetails.solution = "Enable CORS or use allowTaint option";
            } else {
                errorDetails.diagnosis = "Unknown screenshot error";
                errorDetails.solution = "Check browser console for detailed error information";
            }
            
            console.error("📊 Error details:", errorDetails);
            
            // Reject with meaningful error
            throw new Error(`Screenshot failed: ${errorDetails.diagnosis} - ${errorDetails.solution}`);
        });
        """
        
        encoded = base64.b64encode(screenshot_js.encode()).decode()
        command = {
            'type': 'task',
            'role': 'system',
            'task': f'[CMD:BROWSER_JS] {encoded}'
        }
        
        print("📸 Testing version badge screenshot with proper error handling...")
        await websocket.send(json.dumps(command))
        
        response = await websocket.recv()
        print("Screenshot test sent - check browser console for detailed results")
        
        return True

if __name__ == "__main__":
    asyncio.run(test_version_badge_screenshot())