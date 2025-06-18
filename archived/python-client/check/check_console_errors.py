#\!/usr/bin/env python3
"""
Check Console Errors
Direct console monitoring for JavaScript errors
"""

import asyncio
import sys
from pathlib import Path

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient  
from continuum_client.utils import load_continuum_config

async def check_console_errors():
    print("🔍 CHECKING CONSOLE ERRORS")
    
    load_continuum_config()
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'check-console-errors',
            'agentName': 'Check Console Errors',
            'agentType': 'ai'
        })
        
        # Check current console state
        result = await client.js.execute("""
            // Check if we can access the script directly
            var scripts = document.querySelectorAll('script[src*="continuum-api"]');
            console.log('📋 Found continuum-api scripts:', scripts.length);
            
            for (var i = 0; i < scripts.length; i++) {
                console.log('  Script ' + i + ':', scripts[i].src);
            }
            
            // Try to manually evaluate the script content
            try {
                console.log('🧪 Testing manual script evaluation...');
                
                // Force a reload and check errors
                var xhr = new XMLHttpRequest();
                xhr.open('GET', '/src/ui/continuum-api.js', false);
                xhr.send();
                
                if (xhr.status === 200) {
                    console.log('✅ Script content loaded, length:', xhr.responseText.length);
                    
                    // Try to evaluate it
                    try {
                        eval(xhr.responseText);
                        console.log('✅ Script evaluation completed');
                        
                        // Check if function exists now
                        if (typeof initializeContinuum === 'function') {
                            console.log('✅ initializeContinuum function now exists\!');
                            return 'FUNCTION_NOW_EXISTS';
                        } else {
                            console.log('❌ initializeContinuum still not defined after eval');
                            return 'FUNCTION_STILL_MISSING';
                        }
                        
                    } catch (evalError) {
                        console.error('❌ Script evaluation error:', evalError.message);
                        console.error('❌ Error line:', evalError.line);
                        return 'EVAL_ERROR: ' + evalError.message;
                    }
                } else {
                    console.error('❌ Failed to load script:', xhr.status);
                    return 'LOAD_ERROR: ' + xhr.status;
                }
                
            } catch (error) {
                console.error('❌ Manual evaluation exception:', error.message);
                return 'EXCEPTION: ' + error.message;
            }
        """)
        
        print(f"Console check result: {result}")

if __name__ == "__main__":
    asyncio.run(check_console_errors())
EOF < /dev/null