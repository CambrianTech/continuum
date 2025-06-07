#!/usr/bin/env python3
"""
Advanced Deep Space Probe - Sophisticated Interaction Capture

Mission: Complete interaction lifecycle capture with visual analysis
- Before/after visual states of button clicks
- DOM change monitoring during interactions
- Client-side log capture via WebSocket
- Promise-based interaction analysis
- Development cycle debugging support

🛰️ Advanced probe with full interaction analysis capabilities
"""

import asyncio
import json
import base64
import time
from datetime import datetime
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class AdvancedProbe:
    """Advanced Deep Space Probe with sophisticated interaction capture"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
        self.mission_data = {
            'interactions': [],
            'visual_states': [],
            'dom_changes': [],
            'client_logs': []
        }
    
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'advanced-deep-space-probe',
            'agentName': 'Advanced Deep Space Probe',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def capture_visual_state(self, description=""):
        """📸 Capture current visual state with metadata"""
        try:
            capture_js = f"""
            return new Promise((resolve) => {{
                if (typeof html2canvas === 'undefined') {{
                    resolve(JSON.stringify({{success: false, error: 'html2canvas not available'}}));
                    return;
                }}
                
                html2canvas(document.body, {{
                    allowTaint: true,
                    useCORS: true,
                    scale: 0.6,
                    backgroundColor: '#1a1a1a'
                }}).then(canvas => {{
                    const dataURL = canvas.toDataURL('image/jpeg', 0.8);
                    resolve(JSON.stringify({{
                        success: true,
                        dataURL: dataURL,
                        width: canvas.width,
                        height: canvas.height,
                        timestamp: Date.now(),
                        description: '{description}',
                        url: window.location.href,
                        title: document.title
                    }}));
                }}).catch(error => {{
                    resolve(JSON.stringify({{success: false, error: error.message}}));
                }});
            }});
            """
            
            result_json = await self.client.js.get_value(capture_js, timeout=15)
            result = json.loads(result_json)
            
            if result['success']:
                self.mission_data['visual_states'].append(result)
                print(f"📸 Visual state captured: {description} ({result['width']}x{result['height']})")
                return result
            else:
                print(f"❌ Visual capture failed: {result['error']}")
                return None
                
        except Exception as e:
            print(f"❌ Visual capture error: {e}")
            return None
    
    async def setup_dom_monitoring(self):
        """🔍 Setup DOM change monitoring"""
        try:
            monitor_js = """
            return new Promise((resolve) => {
                // Setup DOM monitoring if not already active
                if (window.ADVANCED_PROBE_MONITOR) {
                    resolve('MONITOR_ALREADY_ACTIVE');
                    return;
                }
                
                window.ADVANCED_PROBE_MONITOR = {
                    changes: [],
                    isRecording: false,
                    observer: null,
                    
                    startRecording: function() {
                        this.isRecording = true;
                        this.changes = [];
                        
                        // Setup MutationObserver
                        this.observer = new MutationObserver((mutations) => {
                            if (!this.isRecording) return;
                            
                            mutations.forEach((mutation) => {
                                this.changes.push({
                                    type: mutation.type,
                                    target: mutation.target.tagName + (mutation.target.id ? '#' + mutation.target.id : ''),
                                    timestamp: Date.now()
                                });
                            });
                        });
                        
                        this.observer.observe(document.body, {
                            childList: true,
                            subtree: true,
                            attributes: true,
                            attributeOldValue: true
                        });
                        
                        console.log('🛰️ ADVANCED PROBE: DOM monitoring started');
                    },
                    
                    stopRecording: function() {
                        this.isRecording = false;
                        if (this.observer) {
                            this.observer.disconnect();
                        }
                        console.log('🛰️ ADVANCED PROBE: DOM monitoring stopped');
                        return this.changes;
                    }
                };
                
                resolve('DOM_MONITOR_INSTALLED');
            });
            """
            
            result = await self.client.js.get_value(monitor_js, timeout=10)
            print(f"🔍 DOM monitoring: {result}")
            return result == 'DOM_MONITOR_INSTALLED' or result == 'MONITOR_ALREADY_ACTIVE'
            
        except Exception as e:
            print(f"❌ DOM monitoring setup failed: {e}")
            return False
    
    async def capture_client_logs(self):
        """📋 Capture client-side logs via console override"""
        try:
            log_capture_js = """
            return new Promise((resolve) => {
                // Setup console log capture if not already active
                if (!window.PROBE_LOG_CAPTURE) {
                    window.PROBE_LOG_CAPTURE = {
                        logs: [],
                        originalConsole: {
                            log: console.log,
                            warn: console.warn,
                            error: console.error
                        }
                    };
                    
                    // Override console methods
                    console.log = function(...args) {
                        window.PROBE_LOG_CAPTURE.logs.push({
                            level: 'log',
                            message: args.map(arg => String(arg)).join(' '),
                            timestamp: Date.now()
                        });
                        window.PROBE_LOG_CAPTURE.originalConsole.log.apply(console, args);
                    };
                    
                    console.warn = function(...args) {
                        window.PROBE_LOG_CAPTURE.logs.push({
                            level: 'warn',
                            message: args.map(arg => String(arg)).join(' '),
                            timestamp: Date.now()
                        });
                        window.PROBE_LOG_CAPTURE.originalConsole.warn.apply(console, args);
                    };
                    
                    console.error = function(...args) {
                        window.PROBE_LOG_CAPTURE.logs.push({
                            level: 'error',
                            message: args.map(arg => String(arg)).join(' '),
                            timestamp: Date.now()
                        });
                        window.PROBE_LOG_CAPTURE.originalConsole.error.apply(console, args);
                    };
                }
                
                // Return recent logs
                const recentLogs = window.PROBE_LOG_CAPTURE.logs.slice(-10);
                resolve(JSON.stringify(recentLogs));
            });
            """
            
            logs_json = await self.client.js.get_value(log_capture_js, timeout=5)
            logs = json.loads(logs_json)
            
            self.mission_data['client_logs'].extend(logs)
            print(f"📋 Captured {len(logs)} client-side log entries")
            return logs
            
        except Exception as e:
            print(f"❌ Log capture failed: {e}")
            return []
    
    async def sophisticated_interaction_capture(self, button_selector, description=""):
        """🎯 Complete sophisticated interaction capture cycle"""
        print(f"\n🎯 SOPHISTICATED INTERACTION CAPTURE: {description}")
        print(f"Target: {button_selector}")
        
        interaction_report = {
            'selector': button_selector,
            'description': description,
            'timestamp_start': datetime.now().isoformat(),
            'before_state': None,
            'after_state': None,
            'dom_changes': [],
            'logs_captured': [],
            'interaction_success': False,
            'visual_differences': None
        }
        
        try:
            # Step 1: Capture BEFORE state
            print("  📸 Capturing BEFORE state...")
            before_state = await self.capture_visual_state(f"BEFORE_{description}")
            interaction_report['before_state'] = before_state
            
            # Step 2: Start DOM monitoring
            print("  🔍 Starting DOM monitoring...")
            await self.client.js.get_value("window.ADVANCED_PROBE_MONITOR.startRecording(); return 'RECORDING_STARTED';")
            
            # Step 3: Execute the interaction
            print("  🖱️ Executing interaction...")
            click_js = f"""
            return new Promise((resolve) => {{
                const element = document.querySelector('{button_selector}');
                if (!element) {{
                    resolve(JSON.stringify({{success: false, error: 'Element not found'}}));
                    return;
                }}
                
                console.log('🛰️ ADVANCED PROBE: Clicking element:', element.textContent);
                
                // Scroll into view
                element.scrollIntoView({{ behavior: 'smooth', block: 'center' }});
                
                setTimeout(() => {{
                    try {{
                        element.click();
                        resolve(JSON.stringify({{
                            success: true,
                            elementText: element.textContent.trim(),
                            timestamp: Date.now()
                        }}));
                    }} catch (error) {{
                        resolve(JSON.stringify({{success: false, error: error.message}}));
                    }}
                }}, 500);
            }});
            """
            
            click_result_json = await self.client.js.get_value(click_js, timeout=10)
            click_result = json.loads(click_result_json)
            
            if click_result['success']:
                print(f"  ✅ Interaction executed on: {click_result['elementText']}")
                interaction_report['interaction_success'] = True
            else:
                print(f"  ❌ Interaction failed: {click_result['error']}")
            
            # Step 4: Wait for UI changes to settle
            print("  ⏳ Waiting for UI changes to settle...")
            await asyncio.sleep(2)
            
            # Step 5: Capture AFTER state
            print("  📸 Capturing AFTER state...")
            after_state = await self.capture_visual_state(f"AFTER_{description}")
            interaction_report['after_state'] = after_state
            
            # Step 6: Stop DOM monitoring and get changes
            print("  🔍 Collecting DOM changes...")
            dom_changes_json = await self.client.js.get_value(
                "return JSON.stringify(window.ADVANCED_PROBE_MONITOR.stopRecording());"
            )
            dom_changes = json.loads(dom_changes_json)
            interaction_report['dom_changes'] = dom_changes
            print(f"  📊 Captured {len(dom_changes)} DOM changes")
            
            # Step 7: Capture client logs
            print("  📋 Capturing client-side logs...")
            logs = await self.capture_client_logs()
            interaction_report['logs_captured'] = logs
            
            # Step 8: Analyze visual differences
            if before_state and after_state:
                visual_diff = {
                    'size_change': {
                        'before': f"{before_state['width']}x{before_state['height']}",
                        'after': f"{after_state['width']}x{after_state['height']}"
                    },
                    'data_size_change': len(after_state['dataURL']) - len(before_state['dataURL']),
                    'timestamp_diff': after_state['timestamp'] - before_state['timestamp']
                }
                interaction_report['visual_differences'] = visual_diff
                print(f"  📊 Visual analysis: {visual_diff['data_size_change']} bytes change")
            
            # Step 9: Final status
            interaction_report['timestamp_end'] = datetime.now().isoformat()
            self.mission_data['interactions'].append(interaction_report)
            
            print(f"  🎉 Interaction capture complete!")
            return interaction_report
            
        except Exception as e:
            print(f"  ❌ Interaction capture failed: {e}")
            interaction_report['error'] = str(e)
            return interaction_report
    
    async def analyze_interaction_impact(self, interaction_report):
        """🔬 Analyze the impact of the interaction"""
        print(f"\n🔬 INTERACTION IMPACT ANALYSIS")
        
        if not interaction_report['interaction_success']:
            print("❌ Interaction failed - no analysis possible")
            return
        
        print(f"📊 Analysis for: {interaction_report['description']}")
        
        # DOM Changes Analysis
        dom_changes = interaction_report['dom_changes']
        if dom_changes:
            print(f"🔍 DOM Changes: {len(dom_changes)} mutations detected")
            change_types = {}
            for change in dom_changes:
                change_types[change['type']] = change_types.get(change['type'], 0) + 1
            for change_type, count in change_types.items():
                print(f"   - {change_type}: {count} changes")
        else:
            print("🔍 DOM Changes: No mutations detected")
        
        # Visual Changes Analysis
        visual_diff = interaction_report['visual_differences']
        if visual_diff:
            print(f"📸 Visual Changes:")
            print(f"   - Size: {visual_diff['size_change']['before']} → {visual_diff['size_change']['after']}")
            print(f"   - Data size change: {visual_diff['data_size_change']} bytes")
            print(f"   - Processing time: {visual_diff['timestamp_diff']}ms")
        
        # Logs Analysis
        logs = interaction_report['logs_captured']
        if logs:
            print(f"📋 Client Logs: {len(logs)} entries")
            for log in logs[-3:]:  # Show last 3 logs
                print(f"   - [{log['level']}] {log['message'][:50]}...")
        else:
            print("📋 Client Logs: No recent log entries")
        
        return {
            'dom_mutations': len(dom_changes),
            'visual_changed': visual_diff['data_size_change'] != 0 if visual_diff else False,
            'logs_generated': len(logs),
            'success': interaction_report['interaction_success']
        }

async def advanced_probe_mission():
    """🚀 Main advanced probe mission"""
    print("🛰️ ADVANCED DEEP SPACE PROBE MISSION")
    print("=" * 50)
    print("Mission: Sophisticated interaction capture with complete analysis")
    
    async with AdvancedProbe() as probe:
        # Phase 1: Setup monitoring systems
        print("\n🔧 PHASE 1: Setting up monitoring systems")
        await probe.setup_dom_monitoring()
        await probe.capture_client_logs()  # Initialize log capture
        
        # Phase 2: Capture initial state
        print("\n📸 PHASE 2: Capturing initial probe state")
        initial_state = await probe.capture_visual_state("INITIAL_PROBE_STATE")
        
        # Phase 3: Find interactive elements
        print("\n🎯 PHASE 3: Scanning for interactive elements")
        elements_json = await probe.client.js.get_value("""
        return JSON.stringify(
            Array.from(document.querySelectorAll('button')).slice(0, 5).map((btn, i) => ({
                index: i,
                selector: 'button:nth-of-type(' + (i + 1) + ')',
                text: btn.textContent.trim().substring(0, 40),
                id: btn.id || null,
                classes: btn.className || null,
                visible: btn.offsetWidth > 0 && btn.offsetHeight > 0
            }))
        )
        """)
        
        elements = json.loads(elements_json)
        print(f"Found {len(elements)} interactive elements:")
        for elem in elements:
            print(f"   {elem['index'] + 1}. {elem['text']} (visible: {elem['visible']})")
        
        # Phase 4: Execute sophisticated interaction captures
        print("\n🎯 PHASE 4: Executing sophisticated interaction captures")
        
        test_interactions = [
            ('button:nth-of-type(1)', 'FIRST_BUTTON_INTERACTION'),
            ('button:nth-of-type(2)', 'SECOND_BUTTON_INTERACTION')
        ]
        
        for selector, description in test_interactions:
            # Execute sophisticated capture
            interaction_report = await probe.sophisticated_interaction_capture(selector, description)
            
            # Analyze the interaction impact
            analysis = await probe.analyze_interaction_impact(interaction_report)
            
            # Safety check - ensure probe is still responding
            try:
                status = await probe.client.js.get_value("return 'PROBE_STILL_OPERATIONAL'", timeout=3)
                print(f"✅ Probe status: {status}")
            except:
                print("🚨 WARNING: Probe not responding - aborting further interactions")
                break
            
            print("─" * 40)
        
        # Phase 5: Mission summary
        print("\n📊 PHASE 5: Mission Summary")
        print(f"Total interactions captured: {len(probe.mission_data['interactions'])}")
        print(f"Visual states captured: {len(probe.mission_data['visual_states'])}")
        print(f"Client log entries: {len(probe.mission_data['client_logs'])}")
        
        # Save mission data
        mission_summary = {
            'mission_complete': True,
            'timestamp': datetime.now().isoformat(),
            'total_interactions': len(probe.mission_data['interactions']),
            'visual_states': len(probe.mission_data['visual_states']),
            'log_entries': len(probe.mission_data['client_logs'])
        }
        
        print("\n🎉 ADVANCED PROBE MISSION COMPLETE")
        print("All interaction data captured and analyzed via WebSocket telemetry")

if __name__ == "__main__":
    asyncio.run(advanced_probe_mission())