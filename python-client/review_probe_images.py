#!/usr/bin/env python3
"""
Deep Space Probe - Visual Telemetry Review

🛰️ MISSION: Review the actual screenshots captured during drawer diagnostic
- Retrieve the before/after visual states
- Display the captured "planetary" images
- Analyze what the probe actually saw and accomplished

📸 Time to see what our probe discovered on this distant world!
"""

import asyncio
import json
import base64
import tempfile
import subprocess
import os
from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class ProbeImageReview:
    """Review and display probe-captured screenshots"""
    
    def __init__(self):
        load_continuum_config()
        self.client = None
    
    async def __aenter__(self):
        self.client = ContinuumClient()
        await self.client.__aenter__()
        await self.client.register_agent({
            'agentId': 'probe-image-reviewer',
            'agentName': 'Probe Image Reviewer',
            'agentType': 'ai'
        })
        return self
    
    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self.client:
            await self.client.__aexit__(exc_type, exc_val, exc_tb)
    
    async def capture_and_display_current_state(self, description="CURRENT_PROBE_STATE"):
        """📸 Capture current state and display it"""
        print(f"📸 Capturing and displaying: {description}")
        
        capture_js = f"""
        return new Promise((resolve) => {{
            html2canvas(document.body, {{
                allowTaint: true,
                useCORS: true,
                scale: 0.8,
                backgroundColor: '#1a1a1a'
            }}).then(canvas => {{
                const dataURL = canvas.toDataURL('image/png');
                resolve(JSON.stringify({{
                    success: true,
                    dataURL: dataURL,
                    width: canvas.width,
                    height: canvas.height,
                    description: '{description}',
                    timestamp: Date.now()
                }}));
            }});
        }});
        """
        
        try:
            result_json = await self.client.js.get_value(capture_js, timeout=15)
            result = json.loads(result_json)
            
            if result['success']:
                # Extract base64 data and save to file
                base64_data = result['dataURL'].split(',')[1]
                image_bytes = base64.b64decode(base64_data)
                
                # Create temporary file
                with tempfile.NamedTemporaryFile(delete=False, suffix='.png') as temp_file:
                    temp_file.write(image_bytes)
                    temp_path = temp_file.name
                
                print(f"✅ Screenshot captured: {result['width']}x{result['height']}")
                print(f"📁 Saved to: {temp_path}")
                
                # Open the image
                self._open_image(temp_path)
                
                return {
                    'file_path': temp_path,
                    'width': result['width'],
                    'height': result['height'],
                    'description': description
                }
            else:
                print("❌ Screenshot capture failed")
                return None
                
        except Exception as e:
            print(f"❌ Capture error: {e}")
            return None
    
    def _open_image(self, image_path):
        """🖼️ Open image with default system viewer"""
        try:
            if os.name == 'nt':  # Windows
                subprocess.run(['start', str(image_path)], shell=True)
            elif 'darwin' in os.uname().sysname.lower():  # macOS
                subprocess.run(['open', str(image_path)])
            else:  # Linux
                subprocess.run(['xdg-open', str(image_path)])
            print("🖼️ Image opened in default viewer")
        except Exception as e:
            print(f"❌ Error opening image: {e}")
    
    async def inspect_probe_console_button(self):
        """🔍 Inspect the probe console button we created"""
        print("\n🔍 INSPECTING: Probe console button")
        
        inspect_js = """
        return JSON.stringify({
            probeButton: document.querySelector('.probe-console-btn') ? {
                exists: true,
                text: document.querySelector('.probe-console-btn').textContent,
                position: {
                    top: document.querySelector('.probe-console-btn').style.top,
                    right: document.querySelector('.probe-console-btn').style.right
                },
                visible: document.querySelector('.probe-console-btn').offsetWidth > 0
            } : {exists: false},
            
            testDrawer: document.querySelector('.test-drawer') ? {
                exists: true,
                visible: document.querySelector('.test-drawer').offsetWidth > 0,
                transform: document.querySelector('.test-drawer').style.transform
            } : {exists: false},
            
            demoDrawer: document.querySelector('.probe-demo-drawer') ? {
                exists: true,
                visible: document.querySelector('.probe-demo-drawer').offsetWidth > 0,
                position: document.querySelector('.probe-demo-drawer').style.right
            } : {exists: false}
        });
        """
        
        try:
            inspect_json = await self.client.js.get_value(inspect_js, timeout=5)
            inspect_data = json.loads(inspect_json)
            
            print("📊 PROBE ARTIFACTS DETECTED:")
            
            if inspect_data['probeButton']['exists']:
                print(f"   🛰️ Probe Console Button: ACTIVE")
                print(f"      Text: {inspect_data['probeButton']['text']}")
                print(f"      Position: {inspect_data['probeButton']['position']['top']}, {inspect_data['probeButton']['position']['right']}")
                print(f"      Visible: {inspect_data['probeButton']['visible']}")
            else:
                print("   🛰️ Probe Console Button: NOT FOUND")
            
            if inspect_data['testDrawer']['exists']:
                print(f"   📂 Test Drawer: DETECTED")
                print(f"      Visible: {inspect_data['testDrawer']['visible']}")
                print(f"      Transform: {inspect_data['testDrawer']['transform']}")
            
            if inspect_data['demoDrawer']['exists']:
                print(f"   🎬 Demo Drawer: DETECTED")
                print(f"      Visible: {inspect_data['demoDrawer']['visible']}")
                print(f"      Position: {inspect_data['demoDrawer']['position']}")
            
            return inspect_data
            
        except Exception as e:
            print(f"❌ Inspection failed: {e}")
            return None
    
    async def test_probe_button_interaction(self):
        """🖱️ Test the probe button and capture the result"""
        print("\n🖱️ TESTING: Probe button interaction")
        
        # Capture BEFORE state
        before_image = await self.capture_and_display_current_state("BEFORE_PROBE_BUTTON_TEST")
        
        # Click the probe button
        click_js = """
        return new Promise((resolve) => {
            const probeBtn = document.querySelector('.probe-console-btn');
            if (probeBtn) {
                console.log('🛰️ CLICKING: Probe console button');
                probeBtn.click();
                
                setTimeout(() => {
                    resolve(JSON.stringify({
                        success: true,
                        buttonClicked: true,
                        timestamp: Date.now()
                    }));
                }, 1000);
            } else {
                resolve(JSON.stringify({success: false, error: 'Probe button not found'}));
            }
        });
        """
        
        try:
            click_result_json = await self.client.js.get_value(click_js, timeout=10)
            click_result = json.loads(click_result_json)
            
            if click_result['success']:
                print("✅ Probe button clicked successfully")
                
                # Wait a moment for any UI changes
                await asyncio.sleep(2)
                
                # Capture AFTER state
                after_image = await self.capture_and_display_current_state("AFTER_PROBE_BUTTON_TEST")
                
                return {
                    'before': before_image,
                    'after': after_image,
                    'click_result': click_result
                }
            else:
                print(f"❌ Button click failed: {click_result['error']}")
                return None
                
        except Exception as e:
            print(f"❌ Button test failed: {e}")
            return None

async def review_probe_mission():
    """🚀 Review what the probe actually captured visually"""
    print("🛰️ PROBE IMAGE REVIEW MISSION")
    print("=" * 40)
    print("📸 Time to see what our deep space probe actually captured!")
    
    async with ProbeImageReview() as reviewer:
        # Phase 1: Current state inspection
        print("\n📸 PHASE 1: Current Probe State")
        current_state = await reviewer.capture_and_display_current_state("CURRENT_PROBE_WORLD")
        
        # Phase 2: Inspect probe artifacts
        print("\n🔍 PHASE 2: Probe Artifact Detection")
        artifacts = await reviewer.inspect_probe_console_button()
        
        # Phase 3: Interactive test with visual capture
        print("\n🖱️ PHASE 3: Interactive Visual Test")
        interaction_test = await reviewer.test_probe_button_interaction()
        
        if interaction_test:
            before = interaction_test['before']
            after = interaction_test['after']
            
            if before and after:
                print(f"\n📊 VISUAL COMPARISON:")
                print(f"   Before: {before['width']}x{before['height']} - {before['file_path']}")
                print(f"   After:  {after['width']}x{after['height']} - {after['file_path']}")
                print(f"   🖼️ Both images should now be open for visual inspection!")
        
        print(f"\n🎉 PROBE IMAGE REVIEW COMPLETE!")
        print(f"🛰️ Visual telemetry from the distant browser world successfully retrieved!")
        print(f"📸 Screenshots saved and displayed for mission analysis")

if __name__ == "__main__":
    asyncio.run(review_probe_mission())