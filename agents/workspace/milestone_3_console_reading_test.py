#!/usr/bin/env python3
"""
MILESTONE 3: Console Reading Capability Test
==========================================

Test our ability to read and interpret console output from the browser.
Build on the working console capture from MILESTONE 1.
"""

import asyncio
import websockets
import json
import base64
import time
from datetime import datetime

class ConsoleReaderTester:
    def __init__(self, ws_url="ws://localhost:9000"):
        self.ws_url = ws_url
        self.console_entries = []
        self.errors_detected = []
        self.warnings_detected = []
        self.logs_detected = []
        
    async def test_console_reading_capability(self):
        """Test comprehensive console reading and interpretation"""
        print("\n📖 MILESTONE 3: Testing Console Reading Capability")
        print("=" * 60)
        
        try:
            async with websockets.connect(self.ws_url) as websocket:
                
                # Skip connection banner
                try:
                    await asyncio.wait_for(websocket.recv(), timeout=3.0)
                    print(f"📡 Connection established")
                except:
                    pass
                
                # Test comprehensive console reading with various message types
                comprehensive_test_js = '''
                console.log("📖 MILESTONE 3: Console Reading Test Starting");
                
                // Test different error types
                console.error("CRITICAL_ERROR: Database connection failed");
                console.error("SYNTAX_ERROR: Unexpected token in line 42");
                console.error("NETWORK_ERROR: Failed to fetch from API");
                
                // Test different warning types  
                console.warn("PERFORMANCE_WARNING: Slow query detected (2.3s)");
                console.warn("DEPRECATION_WARNING: Function xyz() is deprecated");
                console.warn("MEMORY_WARNING: High memory usage detected");
                
                // Test informational logs
                console.log("INFO: User authentication successful");
                console.log("DEBUG: Component rendered successfully");
                console.log("TRACE: Function call stack depth: 5");
                
                // Test structured data logging
                console.log("DATA:", JSON.stringify({
                    timestamp: new Date().toISOString(),
                    level: "info",
                    module: "console_reader_test",
                    data: { test: true, milestone: 3 }
                }));
                
                console.log("✅ MILESTONE 3: Console reading test complete");
                
                // Return summary data
                JSON.stringify({
                    testComplete: true,
                    milestone: 3,
                    messagesGenerated: {
                        errors: 3,
                        warnings: 3,
                        logs: 6
                    },
                    timestamp: new Date().toISOString()
                });
                '''
                
                encoded_js = base64.b64encode(comprehensive_test_js.encode()).decode()
                task_message = {
                    'type': 'task',
                    'role': 'system',
                    'task': f'[CMD:BROWSER_JS] {encoded_js}'
                }
                
                print("📤 Sending comprehensive console reading test...")
                await websocket.send(json.dumps(task_message))
                
                # Wait for js_executed response
                for attempt in range(5):
                    try:
                        response = await asyncio.wait_for(websocket.recv(), timeout=8.0)
                        result = json.loads(response)
                        
                        if result.get('type') == 'js_executed' and result.get('success'):
                            print("🎯 Console reading test executed successfully!")
                            
                            self.console_entries = result.get('output', [])
                            test_result = result.get('result', '{}')
                            
                            print(f"📋 Console entries captured: {len(self.console_entries)}")
                            print(f"📊 Test result: {test_result}")
                            
                            if len(self.console_entries) > 0:
                                return self.analyze_console_reading()
                            else:
                                print("❌ No console entries captured")
                                return False
                        
                    except asyncio.TimeoutError:
                        print(f"⏱️ Timeout on attempt {attempt+1}")
                        continue
                
                print("❌ Could not get console reading test results")
                return False
                
        except Exception as e:
            print(f"❌ Console reading test failed: {e}")
            return False
    
    def analyze_console_reading(self):
        """Analyze and categorize the console output"""
        print("\n📊 CONSOLE READING ANALYSIS:")
        print("=" * 50)
        
        # Categorize messages
        for entry in self.console_entries:
            if isinstance(entry, dict):
                level = entry.get('level', 'unknown')
                message = entry.get('message', '')
                
                if level == 'error':
                    self.errors_detected.append(message)
                elif level == 'warn':
                    self.warnings_detected.append(message)
                elif level == 'log':
                    self.logs_detected.append(message)
        
        print(f"🚨 ERRORS DETECTED ({len(self.errors_detected)}):")
        for i, error in enumerate(self.errors_detected):
            print(f"  {i+1}. {error}")
        
        print(f"\n⚠️  WARNINGS DETECTED ({len(self.warnings_detected)}):")
        for i, warning in enumerate(self.warnings_detected):
            print(f"  {i+1}. {warning}")
        
        print(f"\n📝 LOGS DETECTED ({len(self.logs_detected)}):")
        for i, log in enumerate(self.logs_detected):
            print(f"  {i+1}. {log}")
        
        # Test specific reading capabilities
        print(f"\n🔍 READING CAPABILITY TESTS:")
        
        # Test error type detection
        critical_errors = [e for e in self.errors_detected if 'CRITICAL' in e]
        syntax_errors = [e for e in self.errors_detected if 'SYNTAX' in e]
        network_errors = [e for e in self.errors_detected if 'NETWORK' in e]
        
        print(f"  Critical errors detected: {len(critical_errors)} ✅" if critical_errors else "  Critical errors detected: 0 ❌")
        print(f"  Syntax errors detected: {len(syntax_errors)} ✅" if syntax_errors else "  Syntax errors detected: 0 ❌")
        print(f"  Network errors detected: {len(network_errors)} ✅" if network_errors else "  Network errors detected: 0 ❌")
        
        # Test warning type detection
        perf_warnings = [w for w in self.warnings_detected if 'PERFORMANCE' in w]
        dep_warnings = [w for w in self.warnings_detected if 'DEPRECATION' in w]
        mem_warnings = [w for w in self.warnings_detected if 'MEMORY' in w]
        
        print(f"  Performance warnings: {len(perf_warnings)} ✅" if perf_warnings else "  Performance warnings: 0 ❌")
        print(f"  Deprecation warnings: {len(dep_warnings)} ✅" if dep_warnings else "  Deprecation warnings: 0 ❌")
        print(f"  Memory warnings: {len(mem_warnings)} ✅" if mem_warnings else "  Memory warnings: 0 ❌")
        
        # Test structured data reading
        data_logs = [l for l in self.logs_detected if 'DATA:' in l]
        print(f"  Structured data logs: {len(data_logs)} ✅" if data_logs else "  Structured data logs: 0 ❌")
        
        # Test milestone completion detection
        milestone_complete = any('MILESTONE 3: Console reading test complete' in l for l in self.logs_detected)
        print(f"  Milestone completion detected: {'✅' if milestone_complete else '❌'}")
        
        # Overall assessment
        total_expected = 12  # 3 errors + 3 warnings + 6 logs
        total_captured = len(self.console_entries)
        
        print(f"\n📈 OVERALL READING ASSESSMENT:")
        print(f"  Expected messages: {total_expected}")
        print(f"  Captured messages: {total_captured}")
        print(f"  Capture rate: {(total_captured/total_expected)*100:.1f}%")
        
        # Success criteria
        success_criteria = [
            len(self.errors_detected) >= 3,
            len(self.warnings_detected) >= 3,
            len(self.logs_detected) >= 6,
            critical_errors and syntax_errors and network_errors,
            perf_warnings and dep_warnings and mem_warnings,
            data_logs,
            milestone_complete
        ]
        
        passed_criteria = sum(success_criteria)
        total_criteria = len(success_criteria)
        
        print(f"  Success criteria passed: {passed_criteria}/{total_criteria}")
        
        if passed_criteria >= 6:  # Allow 1 failure
            print("🎉 MILESTONE 3 SUCCESS: Console reading capability validated!")
            return True
        else:
            print("❌ Console reading capability needs improvement")
            return False

async def main():
    """Run console reading test for MILESTONE 3"""
    print("🔥 MILESTONE 3: CONSOLE READING CAPABILITY TEST")
    print("=" * 60)
    print("Testing comprehensive console reading and interpretation...")
    
    tester = ConsoleReaderTester()
    success = await tester.test_console_reading_capability()
    
    if success:
        print("\n🎯 MILESTONE 3: CONSOLE READING COMPLETE!")
        print("✅ Ready to proceed to MILESTONE 4")
    else:
        print("\n🔧 MILESTONE 3: NEEDS IMPROVEMENT")
        print("❌ Console reading capability requires enhancement")
        
    return success

if __name__ == "__main__":
    asyncio.run(main())