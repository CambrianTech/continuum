#!/usr/bin/env python3
"""
Debug Browser Validation
Check if browser validation is working and identify silent errors
"""

import asyncio
import sys
import time
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def debug_validation():
    print("🔍 DEBUG: Browser validation check")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-validation',
            'agentName': 'Debug Validation',
            'agentType': 'ai'
        })
        
        # Test 1: Check if browser client is ready
        print("\n--- TEST 1: Browser ready state ---")
        result1 = await client.js.execute("return document.readyState;")
        print(f"Document state: {result1}")
        
        # Test 2: Check WebSocket state
        print("\n--- TEST 2: WebSocket state ---") 
        result2 = await client.js.execute("return window.ws ? window.ws.readyState : 'no ws';")
        print(f"WebSocket state: {result2}")
        
        # Test 3: Check continuum API
        print("\n--- TEST 3: Continuum API state ---")
        result3 = await client.js.execute("return window.continuum ? 'available' : 'missing';")
        print(f"Continuum API: {result3}")
        
        # Test 4: Check html2canvas directly
        print("\n--- TEST 4: Direct html2canvas test ---")
        result4 = await client.js.execute("""
            var el = document.querySelector('.version-badge');
            if (!el) return 'no element';
            
            console.log('Starting direct html2canvas test...');
            
            html2canvas(el, { 
                scale: 1,
                backgroundColor: '#1a1a1a',
                ignoreElements: function(element) {
                    return element.offsetWidth === 0 || element.offsetHeight === 0;
                }
            }).then(function(canvas) {
                console.log('html2canvas success: ' + canvas.width + 'x' + canvas.height);
                return 'html2canvas_success';
            }).catch(function(error) {
                console.error('html2canvas error:', error.message);
                return 'html2canvas_error: ' + error.message;
            });
            
            return 'html2canvas_started';
        """)
        print(f"html2canvas test: {result4}")
        
        # Wait and see what happens
        print("\n⏳ Waiting 5 seconds to see results...")
        time.sleep(5)
        
        # Test 5: Check for any errors in console
        print("\n--- TEST 5: Check for recent errors ---")
        result5 = await client.js.execute("""
            // Check if there are any error listeners or global error handlers
            var errorCount = window.addEventListener ? 'listeners_available' : 'no_listeners';
            console.log('Error handling check: ' + errorCount);
            return errorCount;
        """)
        print(f"Error handling: {result5}")

if __name__ == "__main__":
    asyncio.run(debug_validation())