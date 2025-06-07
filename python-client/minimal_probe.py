#!/usr/bin/env python3
"""
Minimal Deep Space Probe - Safe Mode Operation

Emergency minimal probe with only basic, proven JavaScript
Focus on visual capture and simple interaction monitoring
"""

import asyncio
import json
import base64
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def minimal_probe_mission():
    """🛰️ Minimal probe mission - safe mode only"""
    print("🛰️ MINIMAL PROBE: Emergency safe mode operation")
    print("=" * 50)
    
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'minimal-probe',
            'agentName': 'Minimal Deep Space Probe',
            'agentType': 'ai'
        })
        
        # Test 1: Basic probe response
        print("\n📡 Testing basic probe communication...")
        try:
            response = await client.js.get_value("return 'MINIMAL_PROBE_ONLINE'")
            print(f"✅ Probe responds: {response}")
        except Exception as e:
            print(f"❌ CRITICAL: {e}")
            return
        
        # Test 2: Simple DOM inspection
        print("\n🔍 Inspecting probe environment...")
        try:
            dom_info = await client.js.get_value("""
            return JSON.stringify({
                title: document.title,
                url: window.location.href,
                buttons: document.querySelectorAll('button').length
            })
            """)
            info = json.loads(dom_info)
            print(f"✅ Environment: {info}")
        except Exception as e:
            print(f"❌ DOM inspection failed: {e}")
        
        # Test 3: Safe visual capture
        print("\n📸 Testing visual capture systems...")
        try:
            capture_result = await client.js.get_value("""
            return new Promise((resolve) => {
                if (typeof html2canvas !== 'undefined') {
                    html2canvas(document.body).then(canvas => {
                        resolve(JSON.stringify({
                            success: true,
                            width: canvas.width,
                            height: canvas.height,
                            dataSize: canvas.toDataURL().length
                        }));
                    });
                } else {
                    resolve(JSON.stringify({success: false, error: 'html2canvas not available'}));
                }
            })
            """, timeout=15)
            
            result = json.loads(capture_result)
            if result['success']:
                print(f"✅ Visual capture: {result['width']}x{result['height']}, {result['dataSize']} bytes")
            else:
                print(f"❌ Visual capture failed: {result['error']}")
                
        except Exception as e:
            print(f"❌ Visual capture error: {e}")
        
        # Test 4: Safe button detection
        print("\n🎯 Detecting interactive elements...")
        try:
            button_info = await client.js.get_value("""
            return JSON.stringify(
                Array.from(document.querySelectorAll('button')).slice(0, 3).map(btn => ({
                    text: btn.textContent.trim().substring(0, 30),
                    id: btn.id || 'no-id',
                    className: btn.className || 'no-class'
                }))
            )
            """)
            
            buttons = json.loads(button_info)
            print(f"✅ Found {len(buttons)} interactive elements:")
            for i, btn in enumerate(buttons):
                print(f"   {i+1}. {btn['text']} (id: {btn['id']})")
                
        except Exception as e:
            print(f"❌ Button detection failed: {e}")
        
        # Test 5: Minimal safe click test
        print("\n🖱️  Testing minimal safe click...")
        try:
            click_test = await client.js.get_value("""
            return new Promise((resolve) => {
                const buttons = document.querySelectorAll('button');
                if (buttons.length > 0) {
                    const testButton = buttons[0];
                    const beforeText = testButton.textContent;
                    
                    console.log('PROBE: Testing click on:', beforeText);
                    
                    // Simple click without complex monitoring
                    testButton.click();
                    
                    setTimeout(() => {
                        resolve(JSON.stringify({
                            success: true,
                            buttonText: beforeText,
                            clickExecuted: true,
                            timestamp: Date.now()
                        }));
                    }, 1000);
                } else {
                    resolve(JSON.stringify({success: false, error: 'No buttons found'}));
                }
            })
            """, timeout=10)
            
            result = json.loads(click_test)
            if result['success']:
                print(f"✅ Click test successful on: {result['buttonText']}")
            else:
                print(f"❌ Click test failed: {result['error']}")
                
        except Exception as e:
            print(f"❌ Click test error: {e}")
        
        # Test 6: Probe status verification
        print("\n🔍 Final probe status check...")
        try:
            final_status = await client.js.get_value("return 'PROBE_OPERATIONAL_MINIMAL_MODE'")
            print(f"✅ Final status: {final_status}")
        except Exception as e:
            print(f"❌ Status check failed: {e}")
        
        print("\n🛰️ MINIMAL PROBE MISSION COMPLETE")
        print("Probe remains operational in safe mode")

if __name__ == "__main__":
    asyncio.run(minimal_probe_mission())