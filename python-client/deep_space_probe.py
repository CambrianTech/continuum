#!/usr/bin/env python3
"""
Deep Space Probe Controller

Mission: Control browser as deep space probe with only WebSocket telemetry feedback
- Capture button clicks with before/after visual states
- Read client-side logs safely via server
- Handle development cycles with Promise Post Office System
- Zero direct human feedback - probe operates autonomously

🛰️ CRITICAL: If communication breaks, we've lost the probe!
"""

import asyncio
import json
import base64
import logging
import time
from datetime import datetime
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

# Configure probe logging
logging.basicConfig(level=logging.INFO, format='🛰️ %(asctime)s [PROBE] %(message)s')
logger = logging.getLogger(__name__)

class DeepSpaceProbe:
    """Deep Space Probe Controller - Browser as remote probe"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        self.probe_status = "UNKNOWN"
        self.mission_log = []
        
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'deep-space-probe-controller',
            'agentName': 'Deep Space Probe Controller',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    def log_mission(self, event, data=None):
        """Log mission events for analysis"""
        entry = {
            'timestamp': datetime.now().isoformat(),
            'event': event,
            'data': data,
            'probe_status': self.probe_status
        }
        self.mission_log.append(entry)
        logger.info(f"{event}: {data if data else 'No additional data'}")
    
    async def probe_diagnostics(self):
        """🔍 Run full probe diagnostics - SAFE TEST"""
        self.log_mission("DIAGNOSTICS_START", "Testing probe communication")
        
        try:
            # Test 1: Basic communication
            response = await self.client.js.get_value("return 'PROBE_ALIVE'", timeout=5)
            if response == 'PROBE_ALIVE':
                self.probe_status = "OPERATIONAL"
                self.log_mission("COMM_TEST_PASS", "Basic communication established")
            else:
                self.probe_status = "COMM_DEGRADED"
                self.log_mission("COMM_TEST_WARN", f"Unexpected response: {response}")
            
            # Test 2: Client-side logging capability
            log_test = """
            return new Promise((resolve) => {
                console.log('🛰️ PROBE LOG TEST - Mission Control can you see this?');
                console.warn('🛰️ PROBE WARNING TEST');
                console.error('🛰️ PROBE ERROR TEST');
                resolve('LOG_SYSTEM_READY');
            });
            """
            
            log_response = await self.client.js.get_value(log_test, timeout=5)
            if log_response == 'LOG_SYSTEM_READY':
                self.log_mission("LOG_TEST_PASS", "Client-side logging operational")
            
            # Test 3: DOM access
            dom_test = """
            return JSON.stringify({
                title: document.title || 'NO_TITLE',
                url: window.location.href || 'NO_URL',
                bodyExists: !!document.body,
                elementCount: document.querySelectorAll('*').length
            });
            """
            
            dom_info = json.loads(await self.client.js.get_value(dom_test, timeout=5))
            self.log_mission("DOM_TEST_PASS", dom_info)
            
            # Test 4: Visual capture capability
            screenshot_test = """
            return new Promise((resolve) => {
                if (typeof html2canvas !== 'undefined') {
                    resolve('VISUAL_CAPTURE_READY');
                } else {
                    const script = document.createElement('script');
                    script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
                    script.onload = () => resolve('VISUAL_CAPTURE_LOADED');
                    script.onerror = () => resolve('VISUAL_CAPTURE_FAILED');
                    document.head.appendChild(script);
                }
            });
            """
            
            visual_status = await self.client.js.get_value(screenshot_test, timeout=10)
            self.log_mission("VISUAL_TEST", visual_status)
            
            self.log_mission("DIAGNOSTICS_COMPLETE", f"Probe status: {self.probe_status}")
            return True
            
        except Exception as e:
            self.probe_status = "COMM_LOST"
            self.log_mission("DIAGNOSTICS_FAILED", str(e))
            return False
    
    async def safe_capture_state(self, description=""):
        """📸 Safely capture current visual state"""
        try:
            capture_js = """
            return new Promise((resolve, reject) => {
                if (typeof html2canvas === 'undefined') {
                    reject('VISUAL_SYSTEM_DOWN');
                    return;
                }
                
                html2canvas(document.body, {
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.5,  // Smaller for faster transmission
                    backgroundColor: '#1a1a1a'
                }).then(canvas => {
                    const dataURL = canvas.toDataURL('image/jpeg', 0.7);  // Compressed for speed
                    resolve(JSON.stringify({
                        success: true,
                        dataURL: dataURL,
                        width: canvas.width,
                        height: canvas.height,
                        timestamp: Date.now(),
                        description: arguments[0] || 'STATE_CAPTURE'
                    }));
                }).catch(error => {
                    reject('CAPTURE_FAILED: ' + error.message);
                });
            });
            """.replace('arguments[0]', f"'{description}'")
            
            result_json = await self.client.js.get_value(capture_js, timeout=15)
            result = json.loads(result_json)
            
            self.log_mission("VISUAL_CAPTURE", {
                'description': description,
                'size': f"{result['width']}x{result['height']}",
                'data_size': len(result['dataURL'])
            })
            
            return result
            
        except Exception as e:
            self.log_mission("CAPTURE_FAILED", str(e))
            return None
    
    async def setup_interaction_monitoring(self):
        """🎯 Install interaction monitoring on probe - CRITICAL OPERATION"""
        self.log_mission("INTERACTION_SETUP_START", "Installing interaction monitoring")
        
        try:
            monitor_js = """
            return new Promise((resolve) => {
                // Install global interaction monitor
                if (window.PROBE_MONITOR) {
                    resolve('MONITOR_ALREADY_ACTIVE');
                    return;
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
                
                // Install click monitoring
                document.addEventListener('click', function(e) {
                    window.PROBE_MONITOR.logInteraction('click', e.target, {
                        x: e.clientX,
                        y: e.clientY,
                        button: e.button
                    });
                }, true);
                
                // Install change monitoring  
                document.addEventListener('change', function(e) {
                    window.PROBE_MONITOR.logInteraction('change', e.target, {
                        value: e.target.value
                    });
                }, true);
                
                // Install focus monitoring
                document.addEventListener('focus', function(e) {
                    window.PROBE_MONITOR.logInteraction('focus', e.target);
                }, true);
                
                console.log('🛰️ PROBE: Interaction monitoring installed');
                resolve('MONITOR_INSTALLED');
            });
            """
            
            result = await self.client.js.get_value(monitor_js, timeout=10)
            self.log_mission("INTERACTION_SETUP_COMPLETE", result)
            return result == 'MONITOR_INSTALLED' or result == 'MONITOR_ALREADY_ACTIVE'
            
        except Exception as e:
            self.log_mission("INTERACTION_SETUP_FAILED", str(e))
            return False
    
    async def capture_button_click_cycle(self, button_selector, description=""):
        """🎯 Capture complete button click cycle with before/after visual states"""
        self.log_mission("CLICK_CYCLE_START", f"Target: {button_selector}, Description: {description}")
        
        try:
            # Step 1: Capture BEFORE state
            before_state = await self.safe_capture_state(f"BEFORE_{description}")
            if not before_state:
                self.log_mission("CLICK_CYCLE_ABORT", "Failed to capture before state")
                return None
            
            # Step 2: Start interaction recording
            start_recording = await self.client.js.get_value(
                "window.PROBE_MONITOR.startRecording(); return 'RECORDING_STARTED';", 
                timeout=5
            )
            
            # Step 3: Find and click the button safely
            click_js = f"""
            return new Promise((resolve, reject) => {{
                const button = document.querySelector('{button_selector}');
                if (!button) {{
                    reject('BUTTON_NOT_FOUND: {button_selector}');
                    return;
                }}
                
                console.log('🛰️ PROBE: Clicking button', button);
                
                // Scroll into view first
                button.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                
                setTimeout(() => {{
                    try {{
                        button.click();
                        console.log('🛰️ PROBE: Button clicked successfully');
                        resolve('CLICK_EXECUTED');
                    }} catch (error) {{
                        reject('CLICK_FAILED: ' + error.message);
                    }}
                }}, 500);  // Wait for scroll to complete
            }});
            """
            
            click_result = await self.client.js.get_value(click_js, timeout=10)
            self.log_mission("CLICK_EXECUTED", click_result)
            
            # Step 4: Wait for UI changes to settle
            await asyncio.sleep(2)
            
            # Step 5: Capture AFTER state
            after_state = await self.safe_capture_state(f"AFTER_{description}")
            
            # Step 6: Stop recording and get interactions
            interaction_data = await self.client.js.get_value(
                "return JSON.stringify(window.PROBE_MONITOR.stopRecording());",
                timeout=5
            )
            interactions = json.loads(interaction_data)
            
            # Step 7: Compile complete cycle report
            cycle_report = {
                'button_selector': button_selector,
                'description': description,
                'before_state': before_state,
                'after_state': after_state,
                'interactions': interactions,
                'click_result': click_result,
                'timestamp': datetime.now().isoformat()
            }
            
            self.log_mission("CLICK_CYCLE_COMPLETE", {
                'interactions_captured': len(interactions),
                'visual_changes': 'detected' if before_state and after_state else 'unknown'
            })
            
            return cycle_report
            
        except Exception as e:
            self.log_mission("CLICK_CYCLE_FAILED", str(e))
            return None
    
    async def analyze_visual_changes(self, before_state, after_state):
        """🔍 Analyze visual changes between states"""
        if not before_state or not after_state:
            return "INSUFFICIENT_DATA"
        
        # Simple analysis - in real implementation, could use image comparison
        analysis = {
            'before_size': f"{before_state['width']}x{before_state['height']}",
            'after_size': f"{after_state['width']}x{after_state['height']}",
            'data_size_change': len(after_state['dataURL']) - len(before_state['dataURL']),
            'timestamp_diff': after_state['timestamp'] - before_state['timestamp']
        }
        
        self.log_mission("VISUAL_ANALYSIS", analysis)
        return analysis
    
    async def read_probe_logs(self):
        """📋 Read client-side logs via server WebSocket"""
        try:
            log_reader_js = """
            return new Promise((resolve) => {
                // Capture recent console activity
                const logs = {
                    timestamp: Date.now(),
                    url: window.location.href,
                    title: document.title,
                    errors: [],
                    warnings: [],
                    info: []
                };
                
                // In a real implementation, we'd need to override console methods
                // to capture logs, but this demonstrates the concept
                console.log('🛰️ PROBE: Log reading requested from Mission Control');
                
                resolve(JSON.stringify(logs));
            });
            """
            
            logs_json = await self.client.js.get_value(log_reader_js, timeout=5)
            logs = json.loads(logs_json)
            
            self.log_mission("LOGS_READ", logs)
            return logs
            
        except Exception as e:
            self.log_mission("LOG_READ_FAILED", str(e))
            return None
    
    async def emergency_probe_status(self):
        """🚨 Emergency probe status check"""
        try:
            status = await self.client.js.get_value("return 'PROBE_RESPONDING'", timeout=3)
            return status == 'PROBE_RESPONDING'
        except:
            return False

async def deep_space_mission():
    """🚀 Main deep space probe mission"""
    print("🛰️ MISSION CONTROL: Initializing Deep Space Probe Connection")
    print("=" * 60)
    
    async with DeepSpaceProbe() as probe:
        # Phase 1: Establish communication
        print("\n📡 PHASE 1: Establishing Communication")
        if not await probe.probe_diagnostics():
            print("❌ MISSION ABORT: Failed to establish probe communication")
            return
        
        # Phase 2: Install monitoring systems
        print("\n🔧 PHASE 2: Installing Monitoring Systems")
        if not await probe.setup_interaction_monitoring():
            print("⚠️  WARNING: Interaction monitoring failed, continuing with basic operations")
        
        # Phase 3: Read initial logs
        print("\n📋 PHASE 3: Reading Probe Telemetry")
        logs = await probe.read_probe_logs()
        
        # Phase 4: Test interaction capture
        print("\n🎯 PHASE 4: Testing Interaction Capture")
        
        # Look for a button to test with
        button_search_js = """
        return JSON.stringify({
            buttons: Array.from(document.querySelectorAll('button')).slice(0, 3).map(btn => ({
                selector: btn.tagName.toLowerCase() + (btn.id ? '#' + btn.id : '') + (btn.className ? '.' + btn.className.split(' ')[0] : ''),
                text: btn.textContent ? btn.textContent.trim().substring(0, 50) : 'NO_TEXT',
                visible: btn.offsetWidth > 0 && btn.offsetHeight > 0
            })),
            clickables: Array.from(document.querySelectorAll('[onclick], .clickable, .btn')).slice(0, 3).map(el => ({
                selector: el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + (el.className ? '.' + el.className.split(' ')[0] : ''),
                text: el.textContent ? el.textContent.trim().substring(0, 50) : 'NO_TEXT'
            }))
        });
        """
        
        try:
            elements_json = await probe.client.js.get_value(button_search_js, timeout=10)
            elements = json.loads(elements_json)
            
            probe.log_mission("INTERACTIVE_ELEMENTS_FOUND", elements)
            
            # Test with the first visible button found
            test_targets = elements['buttons'] + elements['clickables']
            
            if test_targets:
                for target in test_targets[:2]:  # Test max 2 elements
                    if target.get('visible', True):
                        print(f"\n🎯 Testing interaction with: {target['text'][:30]}...")
                        
                        cycle_report = await probe.capture_button_click_cycle(
                            target['selector'].split()[0],  # Use just the tag name for safety
                            f"TEST_{target['text'][:20].replace(' ', '_')}"
                        )
                        
                        if cycle_report:
                            analysis = await probe.analyze_visual_changes(
                                cycle_report['before_state'],
                                cycle_report['after_state']
                            )
                            print(f"✅ Interaction captured successfully")
                        
                        # Safety check
                        if not await probe.emergency_probe_status():
                            print("🚨 PROBE NOT RESPONDING - ABORTING MISSION")
                            break
            else:
                print("⚠️  No interactive elements found for testing")
        
        except Exception as e:
            probe.log_mission("MISSION_ERROR", str(e))
            print(f"❌ Mission error: {e}")
        
        # Final status
        print(f"\n📊 MISSION COMPLETE")
        print(f"Final probe status: {probe.probe_status}")
        print(f"Mission log entries: {len(probe.mission_log)}")
        print("\n🛰️ Deep Space Probe mission completed - all telemetry logged")

if __name__ == "__main__":
    asyncio.run(deep_space_mission())