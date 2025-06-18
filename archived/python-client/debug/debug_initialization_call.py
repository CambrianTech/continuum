#!/usr/bin/env python3
"""
Debug Initialization Call
Check if initializeContinuum is being called and where it's failing
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def debug_initialization_call():
    print("🔍 DEBUGGING CONTINUUM INITIALIZATION")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'debug-initialization-call',
            'agentName': 'Debug Initialization Call',
            'agentType': 'ai'
        })
        
        # Test if initializeContinuum function exists
        print("\n--- CHECK 1: Function existence ---")
        result1 = await client.js.execute("""
            return typeof initializeContinuum === 'function' ? 'FUNCTION_EXISTS' : 'FUNCTION_MISSING';
        """)
        print(f"initializeContinuum function: {result1}")
        
        # Test DOM ready state
        print("\n--- CHECK 2: DOM ready state ---")
        result2 = await client.js.execute("""
            return 'readyState: ' + document.readyState + ', loading: ' + (document.readyState === 'loading');
        """)
        print(f"DOM state: {result2}")
        
        # Test manual call to see what happens
        print("\n--- CHECK 3: Manual function call ---")
        result3 = await client.js.execute("""
            if (typeof initializeContinuum === 'function') {
                try {
                    console.log('🧪 Manually calling initializeContinuum...');
                    initializeContinuum();
                    console.log('✅ initializeContinuum call completed');
                    
                    // Check if window.continuum was created
                    if (typeof window.continuum !== 'undefined') {
                        return 'MANUAL_CALL_SUCCESS';
                    } else {
                        return 'MANUAL_CALL_NO_CONTINUUM';
                    }
                } catch (error) {
                    console.error('❌ initializeContinuum failed:', error.message);
                    return 'MANUAL_CALL_ERROR: ' + error.message;
                }
            } else {
                return 'NO_FUNCTION_TO_CALL';
            }
        """)
        print(f"Manual call result: {result3}")
        
        # Check if continuum exists after manual call
        if 'SUCCESS' in str(result3.get('result', '')):
            print("\n--- CHECK 4: Continuum after manual call ---")
            result4 = await client.js.execute("""
                if (typeof window.continuum !== 'undefined') {
                    var props = Object.keys(window.continuum);
                    return 'CONTINUUM_PROPS: ' + props.join(', ');
                } else {
                    return 'STILL_NO_CONTINUUM';
                }
            """)
            print(f"Continuum properties: {result4}")

if __name__ == "__main__":
    asyncio.run(debug_initialization_call())