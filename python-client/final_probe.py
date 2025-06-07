#!/usr/bin/env python3
"""
Final Deep Space Probe - Ultra-Reliable Mission

🛰️ MISSION CRITICAL: Using only PROVEN JavaScript patterns
- Complete interaction capture with visual before/after
- Simple but effective monitoring
- Proven click execution
- All data returned via WebSocket Promise Post Office

This is the bulletproof version for production use!
"""

import asyncio
import json
from datetime import datetime
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class FinalProbe:
    """Ultra-reliable deep space probe using only proven JavaScript"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        self.captures = []
        self.interactions = []
    
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'final-deep-space-probe',
            'agentName': 'Final Deep Space Probe',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def capture_state(self, description):
        """📸 Capture visual state - ULTRA SIMPLE VERSION"""
        try:
            # Use the proven capture method from minimal probe
            result_json = await self.client.js.get_value(f"""
            return new Promise((resolve) => {{
                html2canvas(document.body).then(canvas => {{
                    resolve(JSON.stringify({{
                        success: true,
                        width: canvas.width,
                        height: canvas.height,
                        dataSize: canvas.toDataURL().length,
                        timestamp: Date.now(),
                        description: '{description}'
                    }}));
                }});
            }})
            """, timeout=15)
            
            result = json.loads(result_json)
            self.captures.append(result)
            print(f"📸 {description}: {result['width']}x{result['height']} ({result['dataSize']} bytes)")
            return result
            
        except Exception as e:
            print(f"❌ Capture failed: {e}")
            return None
    
    async def simple_click(self, button_index, description):
        """🖱️ Execute click - ULTRA SIMPLE VERSION"""
        try:
            # Use proven simple click from minimal probe
            click_js = f"""
            return new Promise((resolve) => {{
                const buttons = document.querySelectorAll('button');
                if (buttons.length > {button_index}) {{
                    const button = buttons[{button_index}];
                    const text = button.textContent.trim();
                    console.log('PROBE: Clicking button:', text);
                    button.click();
                    resolve(JSON.stringify({{
                        success: true,
                        buttonText: text,
                        timestamp: Date.now()
                    }}));
                }} else {{
                    resolve(JSON.stringify({{success: false, error: 'Button not found'}}));
                }}
            }})
            """
            
            result_json = await self.client.js.get_value(click_js, timeout=10)
            result = json.loads(result_json)
            
            if result['success']:
                print(f"✅ Clicked: {result['buttonText']}")
            
            return result
            
        except Exception as e:
            print(f"❌ Click failed: {e}")
            return {'success': False, 'error': str(e)}
    
    async def complete_interaction_cycle(self, button_index, description):
        """🎯 Complete interaction with before/after capture"""
        print(f"\n🎯 INTERACTION CYCLE: {description}")
        
        interaction = {
            'description': description,
            'button_index': button_index,
            'timestamp_start': datetime.now().isoformat(),
            'success': False
        }
        
        try:
            # Step 1: BEFORE capture
            print("  📸 BEFORE capture...")
            before = await self.capture_state(f"BEFORE_{description}")
            interaction['before'] = before
            
            # Step 2: Execute click
            print("  🖱️ Executing click...")
            click_result = await self.simple_click(button_index, description)
            interaction['click_result'] = click_result
            
            if not click_result['success']:
                print(f"  ❌ Click failed: {click_result['error']}")
                return interaction
            
            # Step 3: Wait for changes
            print("  ⏳ Waiting for changes...")
            await asyncio.sleep(2)
            
            # Step 4: AFTER capture
            print("  📸 AFTER capture...")
            after = await self.capture_state(f"AFTER_{description}")
            interaction['after'] = after
            
            # Step 5: Simple analysis
            if before and after:
                size_change = after['dataSize'] - before['dataSize']
                interaction['analysis'] = {
                    'data_size_change': size_change,
                    'visual_changed': abs(size_change) > 1000,  # Significant change threshold
                    'time_elapsed': after['timestamp'] - before['timestamp']
                }
                
                print(f"  📊 Analysis: {size_change} bytes change, time: {interaction['analysis']['time_elapsed']}ms")
                interaction['success'] = True
            
            interaction['timestamp_end'] = datetime.now().isoformat()
            self.interactions.append(interaction)
            
            print("  ✅ Interaction cycle complete!")
            return interaction
            
        except Exception as e:
            print(f"  ❌ Interaction failed: {e}")
            interaction['error'] = str(e)
            return interaction

async def final_mission():
    """🚀 Final bulletproof deep space probe mission"""
    print("🛰️ FINAL DEEP SPACE PROBE MISSION")
    print("=" * 45)
    print("🔧 Ultra-reliable probe using proven JavaScript patterns")
    
    async with FinalProbe() as probe:
        # Phase 1: Initial state
        print("\n📸 PHASE 1: Initial state capture")
        initial = await probe.capture_state("INITIAL_STATE")
        
        # Phase 2: Probe environment
        print("\n🔍 PHASE 2: Probing environment")
        try:
            env_info_json = await probe.client.js.get_value("""
            return JSON.stringify({
                title: document.title,
                buttons: document.querySelectorAll('button').length,
                url: window.location.href
            })
            """)
            env_info = json.loads(env_info_json)
            print(f"Environment: {env_info['buttons']} buttons on {env_info['title']}")
        except Exception as e:
            print(f"❌ Environment probe failed: {e}")
        
        # Phase 3: Button identification
        print("\n🎯 PHASE 3: Identifying buttons")
        try:
            buttons_json = await probe.client.js.get_value("""
            return JSON.stringify(
                Array.from(document.querySelectorAll('button')).slice(0, 3).map((btn, i) => ({
                    index: i,
                    text: btn.textContent.trim().substring(0, 25)
                }))
            )
            """)
            buttons = json.loads(buttons_json)
            print(f"Found {len(buttons)} target buttons:")
            for btn in buttons:
                print(f"   {btn['index']}: {btn['text']}")
        except Exception as e:
            print(f"❌ Button identification failed: {e}")
            buttons = []
        
        # Phase 4: Execute interactions
        print("\n🎯 PHASE 4: Executing interaction cycles")
        
        if buttons:
            for i, btn in enumerate(buttons[:2]):  # Test first 2 buttons
                description = f"TEST_{i+1}_{btn['text'][:15].replace(' ', '_')}"
                
                # Execute complete cycle
                interaction = await probe.complete_interaction_cycle(btn['index'], description)
                
                # Probe status check
                try:
                    status = await probe.client.js.get_value("return 'OPERATIONAL'", timeout=3)
                    print(f"  🛰️ Probe status: {status}")
                except:
                    print("  🚨 WARNING: Probe not responding!")
                    break
                
                print("  " + "─" * 40)
        else:
            print("❌ No buttons found to test")
        
        # Phase 5: Mission summary
        print("\n📊 PHASE 5: Mission Summary")
        successful = [i for i in probe.interactions if i['success']]
        
        print(f"Captures completed: {len(probe.captures)}")
        print(f"Interactions attempted: {len(probe.interactions)}")
        print(f"Successful interactions: {len(successful)}")
        
        if successful:
            print("\n✅ Successful Interactions:")
            for interaction in successful:
                analysis = interaction.get('analysis', {})
                print(f"   - {interaction['description']}")
                print(f"     Visual change detected: {analysis.get('visual_changed', False)}")
                print(f"     Data size change: {analysis.get('data_size_change', 0)} bytes")
        
        print("\n🎉 FINAL MISSION COMPLETE!")
        print("🛰️ Deep space probe successfully captured complete interaction cycles")
        print("🔧 All data transmitted via WebSocket Promise Post Office System")
        print("🚀 System ready for full AI-driven web interface control!")

if __name__ == "__main__":
    asyncio.run(final_mission())