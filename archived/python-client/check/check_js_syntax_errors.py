#!/usr/bin/env python3
"""
Check JavaScript Syntax Errors
Check for JavaScript parsing errors in continuum-api.js
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def check_js_syntax_errors():
    print("🔍 CHECKING JAVASCRIPT SYNTAX ERRORS")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'check-js-syntax-errors',
            'agentName': 'Check JS Syntax Errors',
            'agentType': 'ai'
        })
        
        # Override console.error to catch JavaScript errors
        result = await client.js.execute("""
            console.log('🔍 Setting up error catching...');
            
            // Store original console methods
            var originalError = console.error;
            var originalWarn = console.warn;
            var capturedErrors = [];
            var capturedWarns = [];
            
            // Override console.error to capture errors
            console.error = function() {
                var args = Array.prototype.slice.call(arguments);
                capturedErrors.push(args.join(' '));
                originalError.apply(console, arguments);
            };
            
            // Override console.warn to capture warnings  
            console.warn = function() {
                var args = Array.prototype.slice.call(arguments);
                capturedWarns.push(args.join(' '));
                originalWarn.apply(console, arguments);
            };
            
            // Try to load/parse continuum-api.js content by creating a script element
            try {
                console.log('🧪 Attempting to reload continuum-api.js...');
                
                // Create script element to reload the file
                var script = document.createElement('script');
                script.src = '/src/ui/continuum-api.js?reload=' + Date.now();
                script.onerror = function(e) {
                    console.error('❌ Script load error:', e);
                    capturedErrors.push('Script load error: ' + e.message);
                };
                
                document.head.appendChild(script);
                
                // Wait a moment then check results
                setTimeout(function() {
                    // Restore console methods
                    console.error = originalError;
                    console.warn = originalWarn;
                    
                    console.log('📊 Captured errors:', capturedErrors.length);
                    console.log('📊 Captured warnings:', capturedWarns.length);
                    
                    if (capturedErrors.length > 0) {
                        console.log('❌ ERRORS:', capturedErrors);
                    }
                    
                    if (capturedWarns.length > 0) {
                        console.log('⚠️ WARNINGS:', capturedWarns);
                    }
                }, 1000);
                
                return 'ERROR_CATCHING_SETUP';
                
            } catch (error) {
                // Restore console methods
                console.error = originalError;
                console.warn = originalWarn;
                
                console.error('❌ Exception setting up error catching:', error.message);
                return 'SETUP_EXCEPTION: ' + error.message;
            }
        """)
        
        print(f"Error catching setup: {result}")
        
        # Wait for the script reload to complete
        import time
        time.sleep(2)
        
        # Check what happened
        final_result = await client.js.execute("""
            // Check if initializeContinuum exists now
            var functionExists = typeof initializeContinuum === 'function';
            var continuumExists = typeof window.continuum !== 'undefined';
            
            return 'Function: ' + functionExists + ', Continuum: ' + continuumExists;
        """)
        
        print(f"Final check: {final_result}")

if __name__ == "__main__":
    asyncio.run(check_js_syntax_errors())