#!/usr/bin/env python3
"""
Debug Continuum API Loading
Check if continuum-api.js is loaded and initialized properly
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def debug_continuum_api_loading():
    print("🔍 DEBUGGING CONTINUUM API LOADING")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-continuum-api-loading',
            'agentName': 'Debug Continuum API Loading',
            'agentType': 'ai'
        })
        
        # Check 1: Basic window object
        print("\n--- CHECK 1: Basic window availability ---")
        result1 = await client.js.execute("return typeof window !== 'undefined' ? 'WINDOW_AVAILABLE' : 'NO_WINDOW';")
        print(f"Window object: {result1}")
        
        # Check 2: Check what's in window
        print("\n--- CHECK 2: Window properties ---")
        result2 = await client.js.execute("""
            var props = [];
            for (var prop in window) {
                if (prop.includes('continuum') || prop.includes('html2canvas') || prop.includes('ScreenshotUtils')) {
                    props.push(prop);
                }
            }
            return props.length > 0 ? props.join(', ') : 'NO_RELEVANT_PROPS';
        """)
        print(f"Relevant window properties: {result2}")
        
        # Check 3: Check script loading
        print("\n--- CHECK 3: Script elements ---")
        result3 = await client.js.execute("""
            var scripts = document.querySelectorAll('script[src]');
            var relevant = [];
            for (var i = 0; i < scripts.length; i++) {
                var src = scripts[i].src;
                if (src.includes('continuum-api') || src.includes('ScreenshotUtils') || src.includes('html2canvas')) {
                    relevant.push(src);
                }
            }
            return relevant.length > 0 ? relevant.join(', ') : 'NO_RELEVANT_SCRIPTS';
        """)
        print(f"Relevant script sources: {result3}")
        
        # Check 4: Check console for errors
        print("\n--- CHECK 4: Console errors ---")
        result4 = await client.js.execute("""
            // Override console.error temporarily to catch errors
            var errors = [];
            var originalError = console.error;
            console.error = function(msg) {
                errors.push(msg);
                originalError.apply(console, arguments);
            };
            
            // Try to access continuum
            try {
                var continuumType = typeof window.continuum;
                var screenshotType = typeof window.ScreenshotUtils;
                
                console.error = originalError; // Restore
                
                return 'continuum: ' + continuumType + ', ScreenshotUtils: ' + screenshotType + ', errors: ' + errors.length;
            } catch (e) {
                console.error = originalError; // Restore
                return 'EXCEPTION: ' + e.message;
            }
        """)
        print(f"Type check result: {result4}")
        
        # Check 5: Try manual initialization
        print("\n--- CHECK 5: Manual initialization test ---")
        result5 = await client.js.execute("""
            // Check if the continuum-api.js content is available
            if (typeof window.continuum === 'undefined') {
                console.log('⚠️ window.continuum not found - API not loaded');
                
                // Check if we can find any continuum-related functions
                var foundProps = [];
                for (var prop in window) {
                    if (typeof window[prop] === 'function' && prop.toLowerCase().includes('continuum')) {
                        foundProps.push(prop);
                    }
                }
                
                return 'CONTINUUM_MISSING, found props: ' + foundProps.join(', ');
            } else {
                return 'CONTINUUM_AVAILABLE';
            }
        """)
        print(f"Manual initialization check: {result5}")

if __name__ == "__main__":
    asyncio.run(debug_continuum_api_loading())