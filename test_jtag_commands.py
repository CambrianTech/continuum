#!/usr/bin/env python3
"""
JTAG End-to-End Command Testing
Tests all JTAG capabilities through WebSocket interface
"""

import asyncio
import websockets
import json
import sys

async def test_jtag_commands():
    """Test complete JTAG command execution capabilities"""
    
    print("🧪 JTAG End-to-End Command Testing")
    print("=" * 50)
    
    try:
        # Connect to WebSocket daemon
        ws = await websockets.connect("ws://localhost:9000")
        print("✅ Connected to WebSocket daemon")
        
        # Initialize client
        await ws.send(json.dumps({
            "type": "client_init",
            "data": {
                "userAgent": "JTAG Test Client",
                "url": "test://jtag",
                "timestamp": "2025-06-29T21:55:00.000Z"
            }
        }))
        
        response = await ws.recv()
        init_data = json.loads(response)
        client_id = init_data["data"]["clientId"]
        print(f"✅ Client initialized: {client_id}")
        
        # Test commands systematically
        commands_to_test = [
            {"command": "health", "args": {}, "expected": "system health status"},
            {"command": "screenshot", "args": {}, "expected": "screenshot capability"},
            {"command": "browserJS", "args": {"code": "console.log('JTAG test')"}, "expected": "JavaScript execution"},
            {"command": "preferences", "args": {"action": "get", "key": "ui.theme"}, "expected": "preference retrieval"},
            {"command": "reload", "args": {"target": "browser"}, "expected": "reload capability"}
        ]
        
        results = {}
        
        for test in commands_to_test:
            print(f"\n🚀 Testing command: {test['command']}")
            
            # Send command
            command_msg = {
                "type": "execute_command",
                "data": {
                    "command": test["command"],
                    "args": test["args"]
                },
                "timestamp": "2025-06-29T21:55:00.000Z",
                "clientId": client_id
            }
            
            await ws.send(json.dumps(command_msg))
            print(f"📤 Sent: {test['command']} with args {test['args']}")
            
            # Get response
            try:
                response = await asyncio.wait_for(ws.recv(), timeout=5.0)
                response_data = json.loads(response)
                
                print(f"📥 Response type: {response_data.get('type', 'unknown')}")
                print(f"📊 Success: {response_data.get('data', {}).get('success', 'unknown')}")
                
                if 'error' in response_data.get('data', {}):
                    print(f"❌ Error: {response_data['data']['error']}")
                    results[test['command']] = {'success': False, 'error': response_data['data']['error']}
                else:
                    print(f"✅ Response received for {test['command']}")
                    results[test['command']] = {'success': True, 'data': response_data.get('data')}
                    
            except asyncio.TimeoutError:
                print(f"⏰ Timeout waiting for {test['command']} response")
                results[test['command']] = {'success': False, 'error': 'timeout'}
        
        await ws.close()
        
        # Print summary
        print("\n📊 JTAG COMMAND TEST RESULTS")
        print("=" * 50)
        for cmd, result in results.items():
            status = "✅ PASS" if result['success'] else "❌ FAIL"
            error = f" - {result.get('error', '')}" if not result['success'] else ""
            print(f"{status} {cmd}{error}")
        
        # Overall assessment
        passed = sum(1 for r in results.values() if r['success'])
        total = len(results)
        
        print(f"\n🎯 JTAG READINESS: {passed}/{total} commands working")
        
        if passed == total:
            print("🚀 JTAG FULLY OPERATIONAL - Ready for autonomous development!")
        elif passed >= total * 0.7:
            print("⚡ JTAG MOSTLY OPERATIONAL - Good for development with some limitations")
        else:
            print("🔧 JTAG NEEDS WORK - Several commands not functioning")
            
        return passed, total
        
    except Exception as e:
        print(f"❌ JTAG test failed: {e}")
        return 0, len(commands_to_test)

if __name__ == "__main__":
    passed, total = asyncio.run(test_jtag_commands())
    sys.exit(0 if passed == total else 1)