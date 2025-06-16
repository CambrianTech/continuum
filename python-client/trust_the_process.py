#!/usr/bin/env python3
"""
🚨 TRUST THE PROCESS - Single Function Automation
For fresh Claude/Mark agents following the AGENT DEVELOPMENT PROCESS

Usage:
    python trust_the_process.py               # Full integrity check
    python trust_the_process.py --screenshot  # Just take screenshot
    python trust_the_process.py --validate    # Just run validation
"""

import asyncio
import sys
import os
from pathlib import Path
from datetime import datetime
import subprocess
import json

# Add continuum_client to path
sys.path.insert(0, str(Path(__file__).parent))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config


class TrustTheProcess:
    """
    🚨 TRUST THE PROCESS - Automated Agent Development Methodology
    
    Implements the 6-step Baby Steps Development Cycle:
    1️⃣ Clear old data
    2️⃣ Make small change (handled by caller)
    3️⃣ Bump version (handled by caller)
    4️⃣ Test immediately
    5️⃣ Fix ANY errors
    6️⃣ Commit when stable (handled by caller)
    """
    
    def __init__(self):
        self.base_dir = Path(__file__).parent.parent
        self.screenshots_dir = self.base_dir / '.continuum' / 'screenshots'
        self.logs_dir = self.base_dir / '.continuum' / 'logs'
        
    async def clear_old_data(self):
        """Step 1: Clear old data to avoid cheating/confusion"""
        print("🧹 Step 1: Clearing old data...")
        
        # Clear ALL screenshots to avoid cheating/confusion (per process requirement)
        if self.screenshots_dir.exists():
            screenshots = list(self.screenshots_dir.glob('*.png'))
            for screenshot in screenshots:
                screenshot.unlink()
                print(f"   🗑️ Removed old screenshot: {screenshot.name}")
            print(f"   📁 Screenshots directory: cleared completely")
        
        print("   ✅ Old data cleared")
        
    async def update_shared_documentation(self, status="in_progress", findings=None):
        """Update .continuum/shared/ with current status and findings"""
        print("📝 Updating shared documentation...")
        
        shared_dir = self.base_dir / '.continuum' / 'shared'
        shared_dir.mkdir(parents=True, exist_ok=True)
        
        # Update process status file
        status_file = shared_dir / 'process_status.md'
        timestamp = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        
        status_content = f"""# Process Status - {timestamp}

## Current Status: {status.upper()}

## Locations:
- Screenshots: `.continuum/screenshots/` 
- Logs: `.continuum/logs/browser/`
- Shared notes: `.continuum/shared/`

## Last Integrity Check:
- Timestamp: {timestamp}
- Agent: trust_the_process.py
- Status: {status}

## Findings:
{findings or "Running integrity check..."}

## Quick Commands:
```bash
python python-client/trust_the_process.py    # Full check
python python-client/trust_the_process.py --screenshot  # Quick screenshot
python python-client/trust_the_process.py --validate    # Quick validation
```
"""
        
        with open(status_file, 'w') as f:
            f.write(status_content)
            
        print(f"   📄 Updated: {status_file}")
        print("   ✅ Shared documentation updated")
        
    async def test_immediately(self):
        """Step 4: Test immediately with screenshot + console check + validation"""
        print("🧪 Step 4: Testing immediately...")
        
        success_criteria = {
            'agent_validation': False,
            'screenshot_capture': False, 
            'no_console_errors': False,
            'version_check': False,
            'websocket_connection': False
        }
        
        try:
            load_continuum_config()
            
            async with ContinuumClient() as client:
                print("   🔗 WebSocket connection... ", end="", flush=True)
                await client.register_agent({
                    'agentId': 'trust-the-process',
                    'agentName': 'Trust The Process Validator',
                    'agentType': 'ai'
                })
                success_criteria['websocket_connection'] = True
                print("✅")
                
                # Agent validation
                print("   🤖 Agent validation... ", end="", flush=True)
                validation_result = await client.js.execute("""
                    console.log('🧪 TRUST THE PROCESS: Agent validation test');
                    const versionElement = document.querySelector('.version-badge');
                    const versionText = versionElement ? versionElement.textContent.trim() : 'unknown';
                    return {
                        version: versionText.replace('v', '') || 'unknown',
                        timestamp: Date.now(),
                        userAgent: navigator.userAgent,
                        websocket: !!window.ws && window.ws.readyState === 1
                    };
                """)
                
                if validation_result['success']:
                    success_criteria['agent_validation'] = True
                    version_data = json.loads(validation_result['result'])
                    print(f"✅ (v{version_data['version']})")
                    success_criteria['version_check'] = version_data['version'] != 'unknown'
                else:
                    print("❌")
                
                # Screenshot capture using elegant command API
                print("   📸 Screenshot capture... ", end="", flush=True)
                screenshot_result = await self.capture_screenshot_command_api(client)
                if screenshot_result:
                    success_criteria['screenshot_capture'] = True
                    print("✅")
                else:
                    print("❌")
                
                # Console error check
                print("   🔍 Console error check... ", end="", flush=True)
                error_check = await client.js.execute("""
                    const errors = window.continuumErrors || [];
                    console.log('🔍 TRUST THE PROCESS: Error check, found', errors.length, 'errors');
                    return errors.length;
                """)
                
                if error_check['success']:
                    error_count = int(error_check['result'])
                    if error_count == 0:
                        success_criteria['no_console_errors'] = True
                        print("✅ (0 errors)")
                    else:
                        print(f"⚠️ ({error_count} errors found)")
                else:
                    print("❌")
                    
        except Exception as e:
            print(f"\n❌ Test failed with error: {e}")
            
        return success_criteria
    
    async def capture_screenshot_command_api(self, client):
        """Capture screenshot using working approach while we build elegant command API"""
        try:
            # Use working JS execution for now - will transition to command API when server ready
            result = await client.js.execute("""
                console.log('📸 Capturing users & agents section');
                
                const selectors = [
                    '[id*="agent"]',
                    '[class*="agent"]', 
                    '#sidebar',
                    '.sidebar',
                    '[class*="user"]'
                ];
                
                let targetElement = null;
                let targetSelector = null;
                
                for (const selector of selectors) {
                    const element = document.querySelector(selector);
                    if (element && element.offsetWidth > 0 && element.offsetHeight > 0) {
                        targetElement = element;
                        targetSelector = selector;
                        break;
                    }
                }
                
                if (!targetElement) {
                    targetElement = document.body;
                    targetSelector = 'body';
                }
                
                console.log('📸 Capturing', targetSelector);
                
                return new Promise((resolve) => {
                    html2canvas(targetElement, {
                        allowTaint: true,
                        useCORS: true,
                        scale: 1,
                        backgroundColor: '#1a1a1a'
                    }).then(function(canvas) {
                        const dataURL = canvas.toDataURL('image/png');
                        resolve({
                            success: true,
                            dataURL: dataURL,
                            width: canvas.width,
                            height: canvas.height,
                            selector: targetSelector
                        });
                    }).catch(function(error) {
                        console.error('📸 Screenshot failed:', error);
                        resolve({
                            success: false,
                            error: error.message
                        });
                    });
                });
            """)
            
            if result['success']:
                screenshot_data = json.loads(result['result'])
                if screenshot_data['success']:
                    # Save to .continuum/screenshots/
                    self.screenshots_dir.mkdir(parents=True, exist_ok=True)
                    
                    timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
                    filename = f'integrity_check_{timestamp}.png'
                    screenshot_path = self.screenshots_dir / filename
                    
                    # Extract base64 and save
                    import base64
                    base64_data = screenshot_data['dataURL'].split(',')[1]
                    image_bytes = base64.b64decode(base64_data)
                    
                    with open(screenshot_path, 'wb') as f:
                        f.write(image_bytes)
                    
                    print(f"\n   📁 Screenshot saved: {screenshot_path}")
                    
                    # CRITICAL: Open screenshot for User (required by process)
                    try:
                        import subprocess
                        subprocess.run(['open', str(screenshot_path)], check=False)
                        print(f"   🖼️ Screenshot opened for User: {screenshot_path.name}")
                    except Exception as e:
                        print(f"   ⚠️ Could not auto-open screenshot: {e}")
                        print(f"   📖 MANUAL: Please open {screenshot_path}")
                    
                    return True
                    
            return False
            
        except Exception as e:
            print(f"\n   ❌ Screenshot error: {e}")
            return False
    
    
    async def check_success_criteria(self, criteria):
        """Check all success criteria and provide guidance"""
        print("\n🎯 SUCCESS CRITERIA CHECK:")
        
        all_passed = True
        for criterion, passed in criteria.items():
            status = "✅" if passed else "❌"
            readable_name = criterion.replace('_', ' ').title()
            print(f"   {status} {readable_name}")
            if not passed:
                all_passed = False
        
        if all_passed:
            print("\n🎉 ALL SUCCESS CRITERIA MET!")
            print("   System is stable and ready for next development step")
            return True
        else:
            print("\n⚠️ SOME CRITERIA FAILED")
            print("   🛡️ SAFETY RULE: Fix ANY errors before proceeding")
            print("   Following TRUST THE PROCESS methodology")
            return False
    
    async def full_integrity_check(self):
        """Complete TRUST THE PROCESS workflow"""
        print("🚨 TRUST THE PROCESS - Full Integrity Check")
        print("=" * 50)
        
        await self.clear_old_data()
        await self.update_shared_documentation("testing", "Starting integrity check...")
        
        criteria = await self.test_immediately()
        success = await self.check_success_criteria(criteria)
        
        # Update shared documentation with results
        status = "success" if success else "needs_attention"
        findings = self._format_criteria_findings(criteria)
        await self.update_shared_documentation(status, findings)
        
        print("\n📋 PROCESS COMPLETE")
        if success:
            print("🟢 READY FOR NEXT STEP")
        else:
            print("🔴 SYSTEM NEEDS ATTENTION")
            
        return success
    
    def _format_criteria_findings(self, criteria):
        """Format success criteria for documentation"""
        findings = "## Success Criteria Results:\n"
        for criterion, passed in criteria.items():
            status = "✅ PASS" if passed else "❌ FAIL"
            readable_name = criterion.replace('_', ' ').title()
            findings += f"- {status}: {readable_name}\n"
        return findings
    
    async def quick_screenshot(self):
        """Just take a screenshot for verification"""
        print("📸 Quick Screenshot Capture...")
        
        load_continuum_config()
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'quick-screenshot',
                'agentName': 'Quick Screenshot',
                'agentType': 'ai'
            })
            
            success = await self.capture_screenshot_internal(client)
            if success:
                print("✅ Screenshot captured successfully")
            else:
                print("❌ Screenshot failed")
                
            return success
    
    async def quick_validation(self):
        """Just run agent validation"""
        print("🧪 Quick Validation...")
        
        load_continuum_config()
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'quick-validation',
                'agentName': 'Quick Validation',
                'agentType': 'ai'
            })
            
            result = await client.js.execute("""
                console.log('🧪 Quick validation test');
                return {
                    version: window.continuumVersion || 'unknown',
                    websocket: !!window.ws && window.ws.readyState === 1,
                    timestamp: Date.now()
                };
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                print(f"✅ Validation passed")
                print(f"   Version: {data['version']}")
                print(f"   WebSocket: {'✅' if data['websocket'] else '❌'}")
                return True
            else:
                print("❌ Validation failed")
                return False


async def main():
    """CLI interface for TRUST THE PROCESS"""
    processor = TrustTheProcess()
    
    if len(sys.argv) > 1:
        if '--screenshot' in sys.argv:
            await processor.quick_screenshot()
        elif '--validate' in sys.argv:
            await processor.quick_validation()
        else:
            print("Usage: python trust_the_process.py [--screenshot|--validate]")
            return
    else:
        # Full integrity check
        await processor.full_integrity_check()


if __name__ == "__main__":
    asyncio.run(main())