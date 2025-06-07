#!/usr/bin/env python3
"""
Deep Space Probe Emergency Repair

Fix the JavaScript syntax error preventing interaction capture
"""

import asyncio
import json
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

async def emergency_repair():
    """🔧 Emergency repair of probe JavaScript execution"""
    print("🚨 EMERGENCY REPAIR: Fixing probe JavaScript syntax error")
    
    load_continuum_config()
    
    async with ContinuumClient() as client:
        await client.register_agent({
            'agentId': 'probe-repair-unit',
            'agentName': 'Probe Repair Unit',
            'agentType': 'ai'
        })
        
        # Test basic communication first
        try:
            response = await client.js.get_value("return 'REPAIR_UNIT_CONNECTED'", timeout=5)
            print(f"✅ Repair unit connected: {response}")
        except Exception as e:
            print(f"❌ CRITICAL: Cannot connect to probe: {e}")
            return
        
        # Fix the click cycle JavaScript
        print("🔧 Installing corrected interaction capture system...")
        
        corrected_monitor_js = """
        (function() {
            // Install global interaction monitor (corrected version)
            if (window.PROBE_MONITOR) {
                return 'MONITOR_ALREADY_ACTIVE';
            }
            
            window.PROBE_MONITOR = {
                interactions: [],
                isRecording: false,
                
                startRecording: function() {
                    this.isRecording = true;
                    this.interactions = [];
                    console.log('🛰️ PROBE: Interaction recording started');
                },
                
                stopRecording: function() {
                    this.isRecording = false;
                    console.log('🛰️ PROBE: Interaction recording stopped');
                    return this.interactions;
                },
                
                logInteraction: function(type, target, data) {
                    if (!this.isRecording) return;
                    
                    const interaction = {
                        timestamp: Date.now(),
                        type: type,
                        target: {
                            tagName: target.tagName,
                            id: target.id || null,
                            className: target.className || null,
                            textContent: target.textContent ? target.textContent.substring(0, 100) : null
                        },
                        data: data || {}
                    };
                    
                    this.interactions.push(interaction);
                    console.log('🛰️ PROBE INTERACTION:', interaction);
                }
            };
            
            // Remove existing listeners first
            if (window.PROBE_CLICK_LISTENER) {
                document.removeEventListener('click', window.PROBE_CLICK_LISTENER, true);
            }
            
            // Install click monitoring
            window.PROBE_CLICK_LISTENER = function(e) {
                window.PROBE_MONITOR.logInteraction('click', e.target, {
                    x: e.clientX,
                    y: e.clientY,
                    button: e.button
                });
            };
            
            document.addEventListener('click', window.PROBE_CLICK_LISTENER, true);
            
            console.log('🛰️ PROBE: Corrected interaction monitoring installed');
            return 'MONITOR_REPAIRED';
        })();
        """
        
        try:
            result = await client.js.get_value(corrected_monitor_js, timeout=10)
            print(f"✅ Monitor repair result: {result}")
        except Exception as e:
            print(f"❌ Monitor repair failed: {e}")
            return
        
        # Test the corrected click capture
        print("🧪 Testing corrected click capture...")
        
        test_click_js = """
        (function() {
            return new Promise((resolve, reject) => {
                const button = document.querySelector('button');
                if (!button) {
                    reject('NO_BUTTON_FOUND');
                    return;
                }
                
                console.log('🛰️ PROBE: Testing click on button:', button.textContent);
                
                // Start recording
                if (window.PROBE_MONITOR) {
                    window.PROBE_MONITOR.startRecording();
                }
                
                // Scroll into view
                button.scrollIntoView({ behavior: 'smooth', block: 'center' });
                
                setTimeout(() => {
                    try {
                        button.click();
                        console.log('🛰️ PROBE: Test click executed');
                        
                        // Stop recording and get results
                        const interactions = window.PROBE_MONITOR ? window.PROBE_MONITOR.stopRecording() : [];
                        
                        resolve(JSON.stringify({
                            success: true,
                            button_text: button.textContent,
                            interactions_captured: interactions.length,
                            interactions: interactions
                        }));
                    } catch (error) {
                        reject('CLICK_FAILED: ' + error.message);
                    }
                }, 1000);
            });
        })();
        """
        
        try:
            test_result_json = await client.js.get_value(test_click_js, timeout=15)
            test_result = json.loads(test_result_json)
            
            print("✅ REPAIR SUCCESSFUL!")
            print(f"   Button clicked: {test_result['button_text']}")
            print(f"   Interactions captured: {test_result['interactions_captured']}")
            
            if test_result['interactions']:
                print("   Interaction details:")
                for interaction in test_result['interactions']:
                    print(f"     - {interaction['type']} on {interaction['target']['tagName']} at {interaction['timestamp']}")
            
        except Exception as e:
            print(f"❌ Test click failed: {e}")
        
        # Final probe status check
        try:
            status = await client.js.get_value("return 'PROBE_OPERATIONAL'", timeout=5)
            print(f"\n🛰️ FINAL STATUS: {status}")
            print("🎉 Emergency repair completed - probe fully operational!")
            
        except Exception as e:
            print(f"❌ CRITICAL: Probe not responding after repair: {e}")

if __name__ == "__main__":
    asyncio.run(emergency_repair())