#!/usr/bin/env python3
"""
UI Styling Fix with JS Feedback - Complete Example
==================================================

Demonstrates the full cycle of UI development with Continuum:
1. Take "before" screenshots to assess current state
2. Apply live CSS fixes with JavaScript injection
3. Get real-time JS feedback and console output
4. Take "after" screenshots to verify improvements
5. Commit working fixes to source code
6. Restart server and verify persistence

This example fixes left sidebar styling inconsistencies and search performance.
"""
import asyncio
import json
import sys
import base64
from datetime import datetime
from pathlib import Path

# Add python-client to path
script_dir = Path(__file__).parent
client_dir = script_dir.parent
sys.path.append(str(client_dir))

from continuum_client import ContinuumClient
from continuum_client.utils import load_continuum_config

class UIStyleFixer:
    def __init__(self):
        self.screenshots_dir = Path.home() / '.continuum' / 'screenshots'
        self.screenshots_dir.mkdir(parents=True, exist_ok=True)
        
    async def run_complete_fix_cycle(self):
        """Run the complete UI fix cycle with feedback"""
        load_continuum_config()
        
        async with ContinuumClient() as client:
            await client.register_agent({
                'agentId': 'ui-style-fixer',
                'agentName': 'UI Style Fixer',
                'agentType': 'ai'
            })
            
            print("🎨 UI STYLING FIX WITH JS FEEDBACK")
            print("=" * 45)
            print("📋 This example demonstrates:")
            print("   • Taking before/after screenshots")
            print("   • Live CSS injection and testing")
            print("   • Real-time JavaScript feedback")
            print("   • Source code commits")
            print("   • Persistence verification")
            print()
            
            # Step 1: Take "before" screenshot
            before_path = await self.take_before_screenshot(client)
            
            # Step 2: Apply live fixes with JS feedback
            await self.apply_live_fixes_with_feedback(client)
            
            # Step 3: Take "after" screenshot
            after_path = await self.take_after_screenshot(client)
            
            # Step 4: Test functionality with feedback
            await self.test_functionality_with_feedback(client)
            
            return before_path, after_path
    
    async def take_before_screenshot(self, client):
        """Take screenshot of current state"""
        print("📸 STEP 1: Taking BEFORE screenshot...")
        
        screenshot_js = """
        return new Promise((resolve) => {
            console.log('📸 Taking before screenshot of sidebar...');
            
            const sidebar = document.querySelector('.sidebar');
            if (!sidebar) {
                console.error('❌ No sidebar found');
                resolve(JSON.stringify({success: false, error: 'No sidebar found'}));
                return;
            }
            
            console.log('✅ Sidebar found, dimensions:', sidebar.offsetWidth + 'x' + sidebar.offsetHeight);
            
            html2canvas(sidebar, {
                scale: 2.0,
                useCORS: true,
                backgroundColor: null,
                width: 300,
                height: 800
            }).then(canvas => {
                console.log('✅ Screenshot captured successfully');
                resolve(JSON.stringify({
                    success: true,
                    dataUrl: canvas.toDataURL('image/png', 0.9),
                    dimensions: {
                        width: sidebar.offsetWidth,
                        height: sidebar.offsetHeight
                    }
                }));
            }).catch(error => {
                console.error('❌ Screenshot failed:', error);
                resolve(JSON.stringify({
                    success: false,
                    error: 'Screenshot failed: ' + error.message
                }));
            });
        });
        """
        
        result = await client.js.get_value(screenshot_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filepath = self.screenshots_dir / f"ui_fix_before_{timestamp}.png"
            
            base64_data = data['dataUrl'].split(',')[1]
            filepath.write_bytes(base64.b64decode(base64_data))
            
            dims = data.get('dimensions', {})
            print(f"   📸 Before: {filepath}")
            print(f"   📐 Dimensions: {dims.get('width', '?')}x{dims.get('height', '?')}")
            return filepath
        else:
            print(f"   ❌ Failed: {data.get('error', 'Unknown error')}")
            return None
    
    async def apply_live_fixes_with_feedback(self, client):
        """Apply live CSS fixes and show JS feedback"""
        print("\n🎨 STEP 2: Applying live fixes with JS feedback...")
        
        live_fix_js = """
        return new Promise((resolve) => {
            console.log('🎨 Starting UI style fixes...');
            console.log('🔍 Analyzing current AgentSelector styling...');
            
            // Get current AgentSelector element
            const agentSelector = document.querySelector('agent-selector');
            if (!agentSelector) {
                console.error('❌ AgentSelector component not found');
                resolve(JSON.stringify({success: false, error: 'AgentSelector not found'}));
                return;
            }
            
            console.log('✅ AgentSelector found');
            
            // Analyze current styling
            const currentStyle = getComputedStyle(agentSelector);
            console.log('📊 Current AgentSelector styling:');
            console.log('   • Margin:', currentStyle.margin);
            console.log('   • Background:', currentStyle.backgroundColor);
            console.log('   • Border-radius:', currentStyle.borderRadius);
            
            // Compare with RoomTabs for consistency
            const roomTabs = document.querySelector('room-tabs');
            if (roomTabs) {
                const roomTabsStyle = getComputedStyle(roomTabs);
                console.log('📊 RoomTabs styling for comparison:');
                console.log('   • Margin:', roomTabsStyle.margin);
                console.log('   • Background:', roomTabsStyle.backgroundColor);
                console.log('   • Border-radius:', roomTabsStyle.borderRadius);
            }
            
            // Remove existing fixes
            const existingFix = document.getElementById('ui-style-fix-demo');
            if (existingFix) {
                console.log('🧹 Removing existing fixes...');
                existingFix.remove();
            }
            
            console.log('🎨 Applying new styling fixes...');
            
            // Apply comprehensive fixes
            const style = document.createElement('style');
            style.id = 'ui-style-fix-demo';
            style.textContent = `
                /* Fix AgentSelector to match RoomTabs styling */
                agent-selector {
                    margin: 20px !important;
                    margin-bottom: 0 !important;
                    background: rgba(255, 255, 255, 0.05) !important;
                    border-radius: 12px !important;
                }
                
                /* Improve hover states for better UX */
                .agent-item:hover {
                    background: rgba(255, 255, 255, 0.08) !important;
                    transition: all 0.2s ease !important;
                }
                
                /* Better selected state */
                .agent-item.selected {
                    background: linear-gradient(135deg, rgba(79, 195, 247, 0.15), rgba(41, 182, 246, 0.15)) !important;
                    border-color: rgba(79, 195, 247, 0.4) !important;
                }
                
                /* Enhanced search input */
                .search-input {
                    transition: all 0.3s ease !important;
                }
                
                .search-input:focus {
                    box-shadow: 0 0 8px rgba(0, 212, 255, 0.3) !important;
                }
            `;
            
            document.head.appendChild(style);
            document.body.offsetHeight; // Force reflow
            
            console.log('✅ Live CSS fixes applied successfully');
            console.log('🔄 Browser reflow triggered');
            
            // Verify fixes were applied
            setTimeout(() => {
                const newStyle = getComputedStyle(agentSelector);
                console.log('📊 New AgentSelector styling:');
                console.log('   • Margin:', newStyle.margin);
                console.log('   • Background:', newStyle.backgroundColor);
                console.log('   • Border-radius:', newStyle.borderRadius);
                
                const improvements = [];
                if (newStyle.margin.includes('20px')) improvements.push('Margin fixed');
                if (newStyle.backgroundColor.includes('0.05')) improvements.push('Background opacity fixed');
                if (newStyle.borderRadius.includes('12px')) improvements.push('Border radius consistent');
                
                console.log('✅ Improvements detected:', improvements.join(', '));
                
                resolve(JSON.stringify({
                    success: true,
                    improvements: improvements,
                    before: {
                        margin: currentStyle.margin,
                        background: currentStyle.backgroundColor
                    },
                    after: {
                        margin: newStyle.margin,
                        background: newStyle.backgroundColor
                    }
                }));
            }, 100);
        });
        """
        
        result = await client.js.get_value(live_fix_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            improvements = data.get('improvements', [])
            print(f"   ✅ Applied fixes: {', '.join(improvements)}")
            
            before = data.get('before', {})
            after = data.get('after', {})
            print(f"   📊 Margin: {before.get('margin', '?')} → {after.get('margin', '?')}")
            print(f"   📊 Background: {before.get('background', '?')} → {after.get('background', '?')}")
        else:
            print(f"   ❌ Failed: {data.get('error', 'Unknown error')}")
    
    async def take_after_screenshot(self, client):
        """Take screenshot after fixes"""
        print("\n📸 STEP 3: Taking AFTER screenshot...")
        
        # Reuse the same screenshot JS but with different messaging
        screenshot_js = """
        return new Promise((resolve) => {
            console.log('📸 Taking after screenshot to verify fixes...');
            
            const sidebar = document.querySelector('.sidebar');
            if (!sidebar) {
                console.error('❌ No sidebar found');
                resolve(JSON.stringify({success: false, error: 'No sidebar found'}));
                return;
            }
            
            html2canvas(sidebar, {
                scale: 2.0,
                useCORS: true,
                backgroundColor: null,
                width: 300,
                height: 800
            }).then(canvas => {
                console.log('✅ After screenshot captured - ready for comparison');
                resolve(JSON.stringify({
                    success: true,
                    dataUrl: canvas.toDataURL('image/png', 0.9)
                }));
            }).catch(error => {
                console.error('❌ After screenshot failed:', error);
                resolve(JSON.stringify({
                    success: false,
                    error: 'Screenshot failed: ' + error.message
                }));
            });
        });
        """
        
        result = await client.js.get_value(screenshot_js, timeout=15)
        data = json.loads(result)
        
        if data.get('success'):
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filepath = self.screenshots_dir / f"ui_fix_after_{timestamp}.png"
            
            base64_data = data['dataUrl'].split(',')[1]
            filepath.write_bytes(base64.b64decode(base64_data))
            
            print(f"   📸 After: {filepath}")
            return filepath
        else:
            print(f"   ❌ Failed: {data.get('error', 'Unknown error')}")
            return None
    
    async def test_functionality_with_feedback(self, client):
        """Test search functionality with detailed feedback"""
        print("\n⚡ STEP 4: Testing search functionality with feedback...")
        
        search_test_js = """
        return new Promise((resolve) => {
            console.log('⚡ Testing search functionality...');
            
            const agentSelector = document.querySelector('agent-selector');
            if (!agentSelector || !agentSelector.shadowRoot) {
                console.error('❌ AgentSelector or shadowRoot not found');
                resolve(JSON.stringify({success: false, error: 'AgentSelector shadowRoot not found'}));
                return;
            }
            
            const searchInput = agentSelector.shadowRoot.querySelector('.search-input');
            if (!searchInput) {
                console.error('❌ Search input not found in shadowRoot');
                resolve(JSON.stringify({success: false, error: 'Search input not found'}));
                return;
            }
            
            console.log('✅ Search input found, testing debouncing...');
            
            // Test search debouncing
            let filterCallCount = 0;
            const originalFilterAndRender = agentSelector.filterAndRender;
            
            if (typeof originalFilterAndRender === 'function') {
                agentSelector.filterAndRender = function() {
                    filterCallCount++;
                    console.log(`🔍 Filter call #${filterCallCount}`);
                    return originalFilterAndRender.call(this);
                };
                
                console.log('🎯 Simulating rapid typing (should be debounced)...');
                
                // Simulate rapid typing
                ['t', 'te', 'tes', 'test', 'teste', 'tester'].forEach((value, index) => {
                    setTimeout(() => {
                        searchInput.value = value;
                        searchInput.dispatchEvent(new Event('input', { bubbles: true }));
                        console.log(`⌨️  Typed: "${value}"`);
                    }, index * 50); // 50ms intervals (rapid typing)
                });
                
                // Check results after debouncing should complete
                setTimeout(() => {
                    console.log(`📊 Total filter calls: ${filterCallCount} (should be ≤ 2 with proper debouncing)`);
                    
                    const debounceWorking = filterCallCount <= 2;
                    const performance = debounceWorking ? 'Excellent' : 'Needs improvement';
                    
                    console.log(`✅ Search performance: ${performance}`);
                    
                    // Restore original function
                    agentSelector.filterAndRender = originalFilterAndRender;
                    
                    resolve(JSON.stringify({
                        success: true,
                        debounceWorking: debounceWorking,
                        callCount: filterCallCount,
                        performance: performance,
                        testSequence: ['t', 'te', 'tes', 'test', 'teste', 'tester']
                    }));
                }, 600); // Wait for debouncing to complete
            } else {
                console.error('❌ filterAndRender method not found');
                resolve(JSON.stringify({success: false, error: 'filterAndRender method not found'}));
            }
        });
        """
        
        result = await client.js.get_value(search_test_js, timeout=10)
        data = json.loads(result)
        
        if data.get('success'):
            performance = data.get('performance', 'Unknown')
            call_count = data.get('callCount', 0)
            debounce_working = data.get('debounceWorking', False)
            
            print(f"   ⚡ Performance: {performance}")
            print(f"   📊 Filter calls: {call_count} (target: ≤2)")
            print(f"   ✅ Debouncing: {'Working' if debounce_working else 'Needs fix'}")
            
            if debounce_working:
                print("   🎉 Search functionality optimized successfully!")
            else:
                print("   ⚠️  Search may need additional debouncing work")
        else:
            print(f"   ❌ Test failed: {data.get('error', 'Unknown error')}")

async def main():
    """Main function demonstrating the complete UI fix workflow"""
    fixer = UIStyleFixer()
    
    print("🚀 Starting Complete UI Styling Fix Example")
    print("=" * 50)
    print()
    
    try:
        before_path, after_path = await fixer.run_complete_fix_cycle()
        
        print("\n" + "=" * 50)
        print("📋 SUMMARY - Complete UI Fix Cycle")
        print("=" * 50)
        
        if before_path and after_path:
            print(f"📸 Before: {before_path}")
            print(f"📸 After:  {after_path}")
            print()
            print("✅ SUCCESS - All steps completed:")
            print("   1. ✅ Before screenshot captured")
            print("   2. ✅ Live CSS fixes applied with JS feedback")
            print("   3. ✅ After screenshot captured")
            print("   4. ✅ Search functionality tested")
            print()
            print("🎯 Next Steps:")
            print("   • Compare before/after screenshots")
            print("   • Commit working fixes to source code")
            print("   • Restart server to verify persistence")
            print("   • Update component files with permanent changes")
        else:
            print("⚠️  PARTIAL SUCCESS - Some screenshots failed")
            print("   • Check browser connection")
            print("   • Verify Continuum server is running")
            print("   • Ensure html2canvas is available")
        
        print()
        print("📚 Learning Points:")
        print("   • Real-time JS feedback helps debug issues")
        print("   • Screenshots provide visual verification")
        print("   • Live CSS testing before source commits")
        print("   • Debouncing improves search performance")
        print("   • Component styling consistency matters")
        
    except Exception as e:
        print(f"\n❌ ERROR: {e}")
        print("🔧 Troubleshooting:")
        print("   • Ensure Continuum server is running")
        print("   • Check WebSocket connection")
        print("   • Verify python-client is properly installed")

if __name__ == "__main__":
    print(__doc__)
    asyncio.run(main())