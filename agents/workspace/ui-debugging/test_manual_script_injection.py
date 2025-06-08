#!/usr/bin/env python3
"""
Test manual script injection for ChatHeader.js component
Tests whether script injection works to debug component loading issues
"""
import asyncio
import json
import sys
import time
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def test_manual_script_injection():
    """Test manual injection of ChatHeader.js script"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'script-injection-tester',
            'agentName': 'Manual Script Injection Tester',
            'agentType': 'ai'
        })
        
        print("🧪 TESTING MANUAL SCRIPT INJECTION")
        print("=" * 45)
        
        # First, check the current state before injection
        pre_injection_check_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking current state before script injection...');
            
            const currentState = {
                customElementsDefined: typeof customElements !== 'undefined',
                chatHeaderDefined: typeof customElements !== 'undefined' && customElements.get('chat-header') !== undefined,
                chatHeaderInDOM: document.querySelector('chat-header') !== null,
                scriptCount: document.querySelectorAll('script[src]').length,
                chatHeaderScriptExists: Array.from(document.querySelectorAll('script[src]')).some(script => 
                    script.src.includes('ChatHeader.js')
                )
            };
            
            resolve(JSON.stringify({
                success: true,
                state: currentState
            }));
        });
        """
        
        print("🔍 Checking current state before injection...")
        pre_result = await client.js.get_value(pre_injection_check_js, timeout=10)
        pre_data = json.loads(pre_result)
        
        if pre_data.get('success'):
            state = pre_data.get('state', {})
            print(f"\n📊 PRE-INJECTION STATE:")
            print(f"   • customElements defined: {state.get('customElementsDefined', False)}")
            print(f"   • chat-header defined: {state.get('chatHeaderDefined', False)}")
            print(f"   • chat-header in DOM: {state.get('chatHeaderInDOM', False)}")
            print(f"   • Total scripts: {state.get('scriptCount', 0)}")
            print(f"   • ChatHeader.js script exists: {state.get('chatHeaderScriptExists', False)}")
        
        # Now inject the ChatHeader.js script manually
        inject_script_js = """
        return new Promise((resolve) => {
            console.log('💉 Injecting ChatHeader.js script manually...');
            
            // Create a new script element
            const script = document.createElement('script');
            script.type = 'text/javascript';
            script.src = '/ui/components/ChatHeader.js';
            
            let loadStatus = 'pending';
            let errorMessage = null;
            
            // Add load event listener
            script.onload = function() {
                console.log('✅ ChatHeader.js script loaded successfully');
                loadStatus = 'loaded';
                
                // Check if component is now defined
                setTimeout(() => {
                    const isNowDefined = typeof customElements !== 'undefined' && 
                                        customElements.get('chat-header') !== undefined;
                    
                    resolve(JSON.stringify({
                        success: true,
                        loadStatus: loadStatus,
                        componentDefined: isNowDefined,
                        errorMessage: errorMessage
                    }));
                }, 100); // Small delay to allow component registration
            };
            
            // Add error event listener
            script.onerror = function(error) {
                console.error('❌ Error loading ChatHeader.js script:', error);
                loadStatus = 'error';
                errorMessage = 'Script failed to load';
                
                resolve(JSON.stringify({
                    success: true,
                    loadStatus: loadStatus,
                    componentDefined: false,
                    errorMessage: errorMessage
                }));
            };
            
            // Set a timeout in case neither event fires
            setTimeout(() => {
                if (loadStatus === 'pending') {
                    console.warn('⚠️  Script injection timeout');
                    loadStatus = 'timeout';
                    errorMessage = 'Script injection timeout';
                    
                    resolve(JSON.stringify({
                        success: true,
                        loadStatus: loadStatus,
                        componentDefined: false,
                        errorMessage: errorMessage
                    }));
                }
            }, 5000);
            
            // Inject the script into the document head
            document.head.appendChild(script);
            console.log('💉 Script element created and appended to head');
        });
        """
        
        print(f"\n💉 Injecting ChatHeader.js script manually...")
        inject_result = await client.js.get_value(inject_script_js, timeout=10)
        inject_data = json.loads(inject_result)
        
        if inject_data.get('success'):
            load_status = inject_data.get('loadStatus', 'unknown')
            component_defined = inject_data.get('componentDefined', False)
            error_message = inject_data.get('errorMessage')
            
            print(f"   📊 Load Status: {load_status}")
            print(f"   📊 Component Defined: {component_defined}")
            if error_message:
                print(f"   ❌ Error: {error_message}")
        
        # Wait a moment for any async loading to complete
        await asyncio.sleep(2)
        
        # Check final state after injection
        post_injection_check_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking final state after script injection...');
            
            const finalState = {
                customElementsDefined: typeof customElements !== 'undefined',
                chatHeaderDefined: typeof customElements !== 'undefined' && customElements.get('chat-header') !== undefined,
                chatHeaderInDOM: document.querySelector('chat-header') !== null,
                scriptCount: document.querySelectorAll('script[src]').length,
                chatHeaderScriptCount: Array.from(document.querySelectorAll('script[src]')).filter(script => 
                    script.src.includes('ChatHeader.js')
                ).length,
                canCreateChatHeader: false
            };
            
            // Test if we can create a ChatHeader element
            try {
                if (typeof customElements !== 'undefined' && customElements.get('chat-header')) {
                    const testElement = document.createElement('chat-header');
                    finalState.canCreateChatHeader = testElement instanceof HTMLElement;
                }
            } catch (e) {
                finalState.createError = e.message;
            }
            
            // Check console for any new errors
            const errors = [];
            const originalError = console.error;
            console.error = function(...args) {
                errors.push(args.join(' '));
                originalError.apply(console, args);
            };
            
            // Restore console.error
            setTimeout(() => {
                console.error = originalError;
            }, 100);
            
            finalState.newErrors = errors;
            
            resolve(JSON.stringify({
                success: true,
                state: finalState
            }));
        });
        """
        
        print(f"\n🔍 Checking final state after injection...")
        post_result = await client.js.get_value(post_injection_check_js, timeout=10)
        post_data = json.loads(post_result)
        
        if post_data.get('success'):
            final_state = post_data.get('state', {})
            print(f"\n📊 POST-INJECTION STATE:")
            print(f"   • customElements defined: {final_state.get('customElementsDefined', False)}")
            print(f"   • chat-header defined: {final_state.get('chatHeaderDefined', False)}")
            print(f"   • chat-header in DOM: {final_state.get('chatHeaderInDOM', False)}")
            print(f"   • Total scripts: {final_state.get('scriptCount', 0)}")
            print(f"   • ChatHeader.js scripts: {final_state.get('chatHeaderScriptCount', 0)}")
            print(f"   • Can create chat-header: {final_state.get('canCreateChatHeader', False)}")
            
            if final_state.get('createError'):
                print(f"   ❌ Create error: {final_state.get('createError')}")
            
            new_errors = final_state.get('newErrors', [])
            if new_errors:
                print(f"   ❌ New errors during test:")
                for error in new_errors:
                    print(f"      - {error}")
        
        # Test creating an actual ChatHeader element
        test_component_creation_js = """
        return new Promise((resolve) => {
            console.log('🧪 Testing ChatHeader component creation...');
            
            let testResult = {
                success: false,
                error: null,
                elementCreated: false,
                shadowRootExists: false,
                elementConnected: false
            };
            
            try {
                if (typeof customElements !== 'undefined' && customElements.get('chat-header')) {
                    // Create the element
                    const chatHeader = document.createElement('chat-header');
                    testResult.elementCreated = true;
                    
                    // Check if it has shadow DOM
                    if (chatHeader.shadowRoot) {
                        testResult.shadowRootExists = true;
                    }
                    
                    // Temporarily add to DOM to trigger connectedCallback
                    const testContainer = document.createElement('div');
                    testContainer.style.display = 'none';
                    testContainer.appendChild(chatHeader);
                    document.body.appendChild(testContainer);
                    
                    // Wait a moment for connectedCallback
                    setTimeout(() => {
                        testResult.elementConnected = true;
                        testResult.shadowRootExists = chatHeader.shadowRoot !== null;
                        testResult.success = true;
                        
                        // Clean up
                        document.body.removeChild(testContainer);
                        
                        resolve(JSON.stringify(testResult));
                    }, 200);
                } else {
                    testResult.error = 'chat-header custom element not defined';
                    resolve(JSON.stringify(testResult));
                }
            } catch (e) {
                testResult.error = e.message;
                resolve(JSON.stringify(testResult));
            }
        });
        """
        
        print(f"\n🧪 Testing ChatHeader component creation...")
        creation_result = await client.js.get_value(test_component_creation_js, timeout=10)
        creation_data = json.loads(creation_result)
        
        print(f"   📊 Creation test results:")
        print(f"   • Success: {creation_data.get('success', False)}")
        print(f"   • Element created: {creation_data.get('elementCreated', False)}")
        print(f"   • Shadow root exists: {creation_data.get('shadowRootExists', False)}")
        print(f"   • Element connected: {creation_data.get('elementConnected', False)}")
        
        if creation_data.get('error'):
            print(f"   ❌ Error: {creation_data.get('error')}")
        
        return True

async def main():
    """Main function to test manual script injection"""
    print("🧪 Manual Script Injection Tester")
    print("=" * 40)
    print()
    
    try:
        await test_manual_script_injection()
        
        print("\n" + "=" * 45)
        print("📋 SCRIPT INJECTION TEST COMPLETE")
        print("=" * 45)
        print()
        print("🔧 Analysis:")
        print("   • If manual injection works, the issue is with automatic loading")
        print("   • If manual injection fails, check script path and server setup")
        print("   • If component defines but doesn't render, check component code")
        print("   • If script loads but component doesn't define, check for JS errors")
        print()
        print("💡 Next Steps:")
        print("   • Check browser Network tab for 404 errors")
        print("   • Verify script path in UIGenerator.cjs")
        print("   • Test with different script paths")
        print("   • Check if component files exist on server")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print("🔧 Troubleshooting:")
        print("   • Ensure Continuum server is running")
        print("   • Check WebSocket connection")
        print("   • Verify browser has active Continuum tab")
        print("   • Check if ChatHeader.js file exists in src/ui/components/")

if __name__ == "__main__":
    asyncio.run(main())