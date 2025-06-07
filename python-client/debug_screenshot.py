#!/usr/bin/env python3
"""
Debug screenshot functionality
"""

import asyncio
import json
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def debug_screenshot():
    """Debug screenshot capture"""
    
    load_continuum_config()
    
    print("📸 Testing screenshot capture...")
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-screenshot',
            'agentName': 'Debug Screenshot',
            'agentType': 'ai'
        })
        
        print("🔍 Testing simple object return first...")
        
        # Test 1: Simple object return
        simple_js = """
        return {
            test: 'simple',
            number: 42,
            boolean: true
        };
        """
        
        try:
            result = await client.js.get_value(simple_js)
            print(f"Simple result type: {type(result)}")
            print(f"Simple result: {result}")
        except Exception as e:
            print(f"Simple test failed: {e}")
        
        print("\n🔍 Testing Promise resolution...")
        
        # Test 2: Promise resolution
        promise_js = """
        return new Promise((resolve) => {
            setTimeout(() => {
                resolve({
                    test: 'promise',
                    data: 'resolved'
                });
            }, 100);
        });
        """
        
        try:
            result = await client.js.get_value(promise_js)
            print(f"Promise result type: {type(result)}")
            print(f"Promise result: {result}")
        except Exception as e:
            print(f"Promise test failed: {e}")
        
        print("\n🔍 Testing if html2canvas loads...")
        
        # Test 3: Check html2canvas availability
        html2canvas_js = """
        return new Promise((resolve) => {
            if (typeof html2canvas !== 'undefined') {
                resolve({
                    loaded: true,
                    version: 'already available'
                });
            } else {
                const script = document.createElement('script');
                script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                script.onload = () => {
                    resolve({
                        loaded: true,
                        version: 'just loaded'
                    });
                };
                script.onerror = () => {
                    resolve({
                        loaded: false,
                        error: 'Failed to load'
                    });
                };
                document.head.appendChild(script);
            }
        });
        """
        
        try:
            result = await client.js.get_value(html2canvas_js, timeout=15)
            print(f"html2canvas result type: {type(result)}")
            print(f"html2canvas result: {result}")
        except Exception as e:
            print(f"html2canvas test failed: {e}")

if __name__ == "__main__":
    asyncio.run(debug_screenshot())