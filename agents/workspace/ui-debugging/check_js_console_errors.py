#!/usr/bin/env python3
"""
Check JavaScript console for errors and issues that might affect tab management
"""
import asyncio
import json
import sys
import base64
from datetime import datetime
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent.parent.parent / 'python-client'
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def check_js_console_errors():
    """Check for JavaScript console errors and issues"""
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'js-console-checker',
            'agentName': 'JS Console Error Checker',
            'agentType': 'ai'
        })
        
        print("🔍 CHECKING JAVASCRIPT CONSOLE ERRORS")
        print("=" * 45)
        
        # Check for console errors and warnings
        console_check_js = """
        return new Promise((resolve) => {
            console.log('🔍 Checking JavaScript console for errors...');
            
            // Capture existing console methods
            const originalError = console.error;
            const originalWarn = console.warn;
            const originalLog = console.log;
            
            // Arrays to store captured messages
            const errors = [];
            const warnings = [];
            const logs = [];
            
            // Override console methods to capture messages
            console.error = function(...args) {
                errors.push({
                    type: 'error',
                    message: args.join(' '),
                    timestamp: Date.now(),
                    stack: new Error().stack
                });
                originalError.apply(console, args);
            };
            
            console.warn = function(...args) {
                warnings.push({
                    type: 'warning', 
                    message: args.join(' '),
                    timestamp: Date.now()
                });
                originalWarn.apply(console, args);
            };
            
            // Test for common issues
            const issues = [];
            
            // Check WebSocket connection
            if (typeof ws === 'undefined') {
                issues.push('WebSocket (ws) is undefined');
            } else if (ws.readyState !== WebSocket.OPEN) {
                issues.push(`WebSocket state is ${ws.readyState} (should be ${WebSocket.OPEN})`);
            }
            
            // Check for missing components
            const expectedComponents = ['agent-selector', 'chat-area', 'chat-header', 'room-tabs', 'status-pill'];
            expectedComponents.forEach(componentName => {
                const element = document.querySelector(componentName);
                if (!element) {
                    issues.push(`Missing component: ${componentName}`);
                } else if (element.shadowRoot === null && componentName !== 'status-pill') {
                    issues.push(`Component ${componentName} has no shadowRoot`);
                }
            });
            
            // Check for script loading errors
            const scripts = document.querySelectorAll('script[src]');
            scripts.forEach(script => {
                if (script.src && !script.src.startsWith('data:')) {
                    // This is a basic check - actual loading errors would need event listeners
                    if (script.readyState && script.readyState === 'error') {
                        issues.push(`Script loading error: ${script.src}`);
                    }
                }
            });
            
            // Check for missing functions
            const expectedGlobals = ['sendMessage', 'clearChat'];
            expectedGlobals.forEach(funcName => {
                if (typeof window[funcName] !== 'function') {
                    issues.push(`Missing global function: ${funcName}`);
                }
            });
            
            // Check browser capabilities
            const capabilities = {
                webSocket: typeof WebSocket !== 'undefined',
                shadowDOM: typeof Element.prototype.attachShadow !== 'undefined',
                customElements: typeof customElements !== 'undefined',
                promises: typeof Promise !== 'undefined',
                fetch: typeof fetch !== 'undefined',
                localStorage: typeof localStorage !== 'undefined',
                sessionStorage: typeof sessionStorage !== 'undefined'
            };
            
            // Test tab registration
            let tabRegistration = 'unknown';
            try {
                const tabId = sessionStorage.getItem('continuum-tab-id');
                if (tabId) {
                    tabRegistration = `Tab ID: ${tabId}`;
                } else {
                    tabRegistration = 'No tab ID found in sessionStorage';
                }
            } catch (e) {
                tabRegistration = `Error accessing sessionStorage: ${e.message}`;
            }
            
            // Restore original console methods
            console.error = originalError;
            console.warn = originalWarn;
            console.log = originalLog;
            
            resolve(JSON.stringify({
                success: true,
                issues: issues,
                capabilities: capabilities,
                tabRegistration: tabRegistration,
                errorCount: errors.length,
                warningCount: warnings.length,
                errors: errors.slice(0, 5), // Limit to first 5 errors
                warnings: warnings.slice(0, 5), // Limit to first 5 warnings
                timestamp: Date.now()
            }));
        });
        """
        
        print("🔍 Checking for JavaScript console errors and issues...")
        result = await client.js.get_value(console_check_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            issues = data.get('issues', [])
            capabilities = data.get('capabilities', {})
            tab_registration = data.get('tabRegistration', 'Unknown')
            errors = data.get('errors', [])
            warnings = data.get('warnings', [])
            
            print(f"\n📊 RESULTS:")
            print(f"   • Errors found: {data.get('errorCount', 0)}")
            print(f"   • Warnings found: {data.get('warningCount', 0)}")
            print(f"   • Issues detected: {len(issues)}")
            
            print(f"\n📋 Tab Registration: {tab_registration}")
            
            if issues:
                print(f"\n⚠️  ISSUES FOUND:")
                for i, issue in enumerate(issues, 1):
                    print(f"   {i}. {issue}")
            else:
                print(f"\n✅ No critical issues detected")
            
            print(f"\n🔧 BROWSER CAPABILITIES:")
            for capability, supported in capabilities.items():
                status = "✅" if supported else "❌"
                print(f"   {status} {capability}: {supported}")
            
            if errors:
                print(f"\n❌ JAVASCRIPT ERRORS:")
                for i, error in enumerate(errors, 1):
                    print(f"   {i}. {error.get('message', 'Unknown error')}")
                    if error.get('stack'):
                        # Show first line of stack trace
                        stack_lines = error['stack'].split('\n')
                        if len(stack_lines) > 1:
                            print(f"      Stack: {stack_lines[1].strip()}")
            
            if warnings:
                print(f"\n⚠️  JAVASCRIPT WARNINGS:")
                for i, warning in enumerate(warnings, 1):
                    print(f"   {i}. {warning.get('message', 'Unknown warning')}")
        
        # Test WebSocket specifically
        websocket_test_js = """
        return new Promise((resolve) => {
            console.log('🔌 Testing WebSocket connection...');
            
            const wsInfo = {
                defined: typeof ws !== 'undefined',
                readyState: typeof ws !== 'undefined' ? ws.readyState : 'undefined',
                url: typeof ws !== 'undefined' ? ws.url : 'undefined',
                protocol: typeof ws !== 'undefined' ? ws.protocol : 'undefined'
            };
            
            // Test sending a message
            let sendTest = 'not attempted';
            if (typeof ws !== 'undefined' && ws.readyState === WebSocket.OPEN) {
                try {
                    // Don't actually send to avoid affecting the system
                    sendTest = 'WebSocket is ready to send';
                } catch (e) {
                    sendTest = `Send error: ${e.message}`;
                }
            } else {
                sendTest = 'WebSocket not ready for sending';
            }
            
            wsInfo.sendTest = sendTest;
            
            console.log('🔌 WebSocket info:', wsInfo);
            
            resolve(JSON.stringify({
                success: true,
                websocket: wsInfo
            }));
        });
        """
        
        print(f"\n🔌 Testing WebSocket connection...")
        ws_result = await client.js.get_value(websocket_test_js, timeout=10)
        ws_data = json.loads(ws_result)
        
        if ws_data.get('success'):
            ws_info = ws_data.get('websocket', {})
            print(f"   📊 WebSocket defined: {ws_info.get('defined', False)}")
            print(f"   📊 Ready state: {ws_info.get('readyState', 'Unknown')}")
            print(f"   📊 URL: {ws_info.get('url', 'Unknown')}")
            print(f"   📊 Send test: {ws_info.get('sendTest', 'Unknown')}")
        
        return True

async def main():
    """Main function to check JavaScript console"""
    print("🔍 JavaScript Console Error Checker")
    print("=" * 40)
    print()
    
    try:
        await check_js_console_errors()
        
        print("\n" + "=" * 45)
        print("📋 CONSOLE CHECK COMPLETE")
        print("=" * 45)
        print()
        print("🔧 Common Issues and Solutions:")
        print("   • Missing components → Check component loading")
        print("   • WebSocket issues → Verify server connection")
        print("   • Script errors → Check browser dev tools")
        print("   • Permission errors → Check console for details")
        print()
        print("💡 Next Steps:")
        print("   • Open browser dev tools (F12)")
        print("   • Check Console tab for additional errors")
        print("   • Check Network tab for failed requests")
        print("   • Verify all component scripts are loading")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print("🔧 Troubleshooting:")
        print("   • Ensure Continuum server is running")
        print("   • Check WebSocket connection")
        print("   • Verify browser has active Continuum tab")

if __name__ == "__main__":
    asyncio.run(main())