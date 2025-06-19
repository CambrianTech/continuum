#!/usr/bin/env python3
"""
Test direct DevTools connection
"""

import asyncio
import websockets
import json
import requests

async def test_direct_devtools():
    """Test direct connection to DevTools"""
    
    print("🔍 Testing direct DevTools API access...")
    
    # Test HTTP API first
    try:
        response = requests.get("http://localhost:9222/json", timeout=5)
        if response.status_code == 200:
            tabs = response.json()
            print(f"✅ DevTools HTTP API accessible - found {len(tabs)} tabs")
            
            # Find a tab to connect to
            target_tab = None
            for tab in tabs:
                if 'localhost:9000' in tab.get('url', ''):
                    target_tab = tab
                    break
            
            if not target_tab and tabs:
                target_tab = tabs[0]  # Use first tab
            
            if target_tab:
                print(f"🎯 Target tab: {target_tab['title']} - {target_tab['url']}")
                ws_url = target_tab['webSocketDebuggerUrl']
                
                # Test WebSocket connection
                print(f"🔌 Testing WebSocket connection to: {ws_url}")
                
                try:
                    async with websockets.connect(ws_url) as ws:
                        print("✅ WebSocket connected successfully!")
                        
                        # Enable Page domain first
                        page_enable = {
                            "id": 1,
                            "method": "Page.enable",
                            "params": {}
                        }
                        
                        await ws.send(json.dumps(page_enable))
                        print("📤 Sent Page.enable command")
                        
                        # Read messages until we get our response
                        page_response = None
                        for _ in range(5):  # Try up to 5 messages
                            msg = await asyncio.wait_for(ws.recv(), timeout=2)
                            data = json.loads(msg)
                            print(f"📥 Message: {data.get('method', data.get('id', 'unknown'))}")
                            if data.get('id') == 1:
                                page_response = data
                                break
                        
                        if page_response:
                            print("✅ Page.enable successful")
                        
                        # Test screenshot command
                        screenshot_cmd = {
                            "id": 2,
                            "method": "Page.captureScreenshot",
                            "params": {"format": "png"}
                        }
                        
                        await ws.send(json.dumps(screenshot_cmd))
                        print("📤 Sent Page.captureScreenshot command")
                        
                        # Read messages until we get screenshot response
                        screenshot_response = None
                        for _ in range(10):  # Try up to 10 messages
                            msg = await asyncio.wait_for(ws.recv(), timeout=5)
                            data = json.loads(msg)
                            print(f"📥 Message: {data.get('method', data.get('id', 'unknown'))}")
                            if data.get('id') == 2:
                                screenshot_response = data
                                break
                        
                        if screenshot_response and 'result' in screenshot_response and 'data' in screenshot_response['result']:
                            print(f"✅ Screenshot captured! Size: {len(screenshot_response['result']['data'])} chars")
                            return True
                        else:
                            print(f"❌ Screenshot failed: {screenshot_response}")
                            
                except websockets.exceptions.ConnectionClosed:
                    print("❌ WebSocket connection closed")
                except asyncio.TimeoutError:
                    print("❌ WebSocket timeout")
                except Exception as e:
                    print(f"❌ WebSocket error: {e}")
            else:
                print("❌ No suitable tab found")
        else:
            print(f"❌ DevTools HTTP API failed: {response.status_code}")
            
    except requests.exceptions.RequestException as e:
        print(f"❌ HTTP request failed: {e}")
    
    return False

if __name__ == "__main__":
    success = asyncio.run(test_direct_devtools())
    if success:
        print("🎉 Direct DevTools connection works!")
    else:
        print("💥 Direct DevTools connection failed")