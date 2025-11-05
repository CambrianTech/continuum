#!/usr/bin/env python3
"""
Continuum Self-Diagnostics
Comprehensive system health checks that run automatically on connection
"""

import asyncio
import json
import time
from pathlib import Path
from typing import Dict, List, Optional, Any
from datetime import datetime

from ..core.client import ContinuumClient
from ..utils.config import load_continuum_config


class ContinuumDiagnostics:
    """
    Self-diagnostic system for Continuum
    Runs comprehensive health checks across the entire stack
    """
    
    def __init__(self):
        self.results = {}
        self.start_time = None
        self.end_time = None
    
    async def run_full_diagnostics(self, client: Optional[ContinuumClient] = None) -> Dict[str, Any]:
        """
        Run complete diagnostic suite
        
        Args:
            client: Optional existing client, will create one if not provided
            
        Returns:
            Comprehensive diagnostic results
        """
        print("🔧 CONTINUUM SELF-DIAGNOSTICS")
        print("=" * 50)
        
        self.start_time = time.time()
        
        if client:
            return await self._run_with_client(client)
        else:
            load_continuum_config()
            async with ContinuumClient() as client:
                await client.register_agent({
                    'agentId': 'self-diagnostics',
                    'agentName': 'Continuum Self-Diagnostics',
                    'agentType': 'ai'
                })
                return await self._run_with_client(client)
    
    async def _run_with_client(self, client: ContinuumClient) -> Dict[str, Any]:
        """Run diagnostics with an active client"""
        
        # Test 1: Basic Connection Health
        print("📡 Testing basic connection health...")
        self.results['connection'] = await self._test_connection_health(client)
        
        # Test 2: Browser API Availability  
        print("🌐 Testing browser API availability...")
        self.results['browser_api'] = await self._test_browser_api(client)
        
        # Test 3: Console and Logging
        print("📋 Testing console and logging...")
        self.results['console_logs'] = await self._test_console_logging(client)
        
        # Test 4: Version Information
        print("📦 Testing version information...")
        self.results['version_info'] = await self._test_version_info(client)
        
        # Test 5: Screenshot Functionality
        print("📸 Testing screenshot functionality...")
        self.results['screenshots'] = await self._test_screenshot_functionality(client)
        
        # Test 6: WebSocket Communication
        print("🔗 Testing WebSocket communication...")
        self.results['websocket'] = await self._test_websocket_communication(client)
        
        # Test 7: File System Access
        print("📁 Testing file system access...")
        self.results['filesystem'] = await self._test_filesystem_access()
        
        # Test 8: Widget Detection
        print("🎛️ Testing widget detection...")
        self.results['widgets'] = await self._test_widget_detection(client)
        
        self.end_time = time.time()
        
        # Generate summary
        summary = self._generate_summary()
        
        # Save results
        await self._save_diagnostic_results()
        
        return {
            'timestamp': datetime.now().isoformat(),
            'duration': self.end_time - self.start_time,
            'summary': summary,
            'results': self.results
        }
    
    async def _test_connection_health(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test basic connection health"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            # Test basic JS execution
            result = await client.js.execute("""
                return {
                    success: true,
                    timestamp: Date.now(),
                    userAgent: navigator.userAgent,
                    url: window.location.href,
                    readyState: document.readyState
                };
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                test_result['status'] = 'healthy'
                test_result['details'] = {
                    'js_execution': True,
                    'browser_info': data.get('userAgent', 'unknown')[:100],
                    'page_url': data.get('url', 'unknown'),
                    'document_ready': data.get('readyState') == 'complete'
                }
                print("  ✅ Basic connection healthy")
            else:
                test_result['status'] = 'unhealthy'
                test_result['issues'].append('JavaScript execution failed')
                print("  ❌ JavaScript execution failed")
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"Connection test error: {str(e)}")
            print(f"  ❌ Connection test error: {e}")
        
        return test_result
    
    async def _test_browser_api(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test browser API availability"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            result = await client.js.execute("""
                var apiStatus = {
                    continuum: typeof window.continuum !== 'undefined',
                    screenshotUtils: typeof window.ScreenshotUtils !== 'undefined',
                    html2canvas: typeof html2canvas !== 'undefined',
                    websocket: typeof window.ws !== 'undefined'
                };
                
                var details = {};
                
                if (apiStatus.continuum) {
                    details.continuum = {
                        version: window.continuum.version,
                        connected: window.continuum.connected,
                        hasCommand: typeof window.continuum.command !== 'undefined',
                        hasScreenshot: typeof window.continuum.command?.screenshot === 'function'
                    };
                }
                
                if (apiStatus.websocket && window.ws) {
                    details.websocket = {
                        readyState: window.ws.readyState,
                        url: window.ws.url
                    };
                }
                
                return {
                    success: true,
                    apiStatus: apiStatus,
                    details: details
                };
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                api_status = data['apiStatus']
                details = data['details']
                
                test_result['details'] = details
                
                # Check critical APIs
                if api_status['continuum'] and api_status['screenshotUtils']:
                    test_result['status'] = 'healthy'
                    print("  ✅ All critical APIs available")
                else:
                    test_result['status'] = 'degraded'
                    if not api_status['continuum']:
                        test_result['issues'].append('Continuum API not available')
                        print("  ❌ Continuum API missing")
                    if not api_status['screenshotUtils']:
                        test_result['issues'].append('ScreenshotUtils not available')
                        print("  ❌ ScreenshotUtils missing")
                    if not api_status['html2canvas']:
                        test_result['issues'].append('html2canvas not available')
                        print("  ⚠️ html2canvas missing")
                    if not api_status['websocket']:
                        test_result['issues'].append('WebSocket not available')
                        print("  ❌ WebSocket missing")
            else:
                test_result['status'] = 'error'
                test_result['issues'].append('API test execution failed')
                print("  ❌ API test execution failed")
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"API test error: {str(e)}")
            print(f"  ❌ API test error: {e}")
        
        return test_result
    
    async def _test_console_logging(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test console and logging functionality"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            # Generate unique test ID for correlation
            test_id = f"DIAG_{int(time.time())}"
            
            result = await client.js.execute(f"""
                var testId = '{test_id}';
                var logs = [];
                var originalLog = console.log;
                var originalWarn = console.warn;
                var originalError = console.error;
                
                // Capture logs
                function captureLog(level, args) {{
                    logs.push({{
                        level: level,
                        message: Array.prototype.slice.call(args).join(' '),
                        timestamp: Date.now()
                    }});
                }}
                
                console.log = function() {{
                    captureLog('log', arguments);
                    originalLog.apply(console, arguments);
                }};
                
                console.warn = function() {{
                    captureLog('warn', arguments);
                    originalWarn.apply(console, arguments);
                }};
                
                console.error = function() {{
                    captureLog('error', arguments);
                    originalError.apply(console, arguments);
                }};
                
                // Generate test logs
                console.log('🧪 DIAGNOSTIC LOG TEST:', testId);
                console.warn('🧪 DIAGNOSTIC WARN TEST:', testId);
                console.error('🧪 DIAGNOSTIC ERROR TEST:', testId);
                
                // Test continuum API logging if available
                if (typeof window.continuum !== 'undefined') {{
                    console.log('📦 Continuum version:', window.continuum.version);
                }}
                
                // Restore console
                console.log = originalLog;
                console.warn = originalWarn;
                console.error = originalError;
                
                return {{
                    success: true,
                    testId: testId,
                    capturedLogs: logs,
                    logLevels: {{
                        log: logs.filter(l => l.level === 'log').length,
                        warn: logs.filter(l => l.level === 'warn').length,
                        error: logs.filter(l => l.level === 'error').length
                    }}
                }};
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                log_levels = data['logLevels']
                
                test_result['details'] = {
                    'test_id': data['testId'],
                    'total_logs': len(data['capturedLogs']),
                    'log_levels': log_levels
                }
                
                if log_levels['log'] > 0 and log_levels['warn'] > 0 and log_levels['error'] > 0:
                    test_result['status'] = 'healthy'
                    print(f"  ✅ Console logging working ({sum(log_levels.values())} messages)")
                else:
                    test_result['status'] = 'degraded'
                    test_result['issues'].append('Some log levels not captured')
                    print("  ⚠️ Some console log levels missing")
            else:
                test_result['status'] = 'error'
                test_result['issues'].append('Console test execution failed')
                print("  ❌ Console test execution failed")
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"Console test error: {str(e)}")
            print(f"  ❌ Console test error: {e}")
        
        return test_result
    
    async def _test_version_info(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test version information accuracy"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            result = await client.js.execute("""
                var versionData = {
                    continuum_version: null,
                    badge_version: null,
                    document_title: document.title,
                    url: window.location.href
                };
                
                // Get continuum version
                if (typeof window.continuum !== 'undefined') {
                    versionData.continuum_version = window.continuum.version;
                }
                
                // Get version badge
                var badge = document.querySelector('.version-badge');
                if (badge) {
                    versionData.badge_version = badge.textContent.trim();
                    versionData.badge_visible = badge.offsetWidth > 0 && badge.offsetHeight > 0;
                } else {
                    versionData.badge_visible = false;
                }
                
                return {
                    success: true,
                    versionData: versionData
                };
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                version_data = data['versionData']
                
                test_result['details'] = version_data
                
                continuum_version = version_data['continuum_version']
                badge_version = version_data['badge_version']
                
                issues = []
                
                # Check continuum version format
                if continuum_version:
                    import re
                    if not re.match(r'^\d+\.\d+\.\d+', continuum_version):
                        issues.append('Invalid continuum version format')
                    else:
                        print(f"  ✅ Continuum version: {continuum_version}")
                else:
                    issues.append('Continuum version not available')
                    print("  ❌ Continuum version missing")
                
                # Check version badge
                if badge_version and version_data['badge_visible']:
                    print(f"  ✅ Version badge: '{badge_version}'")
                    
                    # Check consistency
                    if continuum_version and badge_version:
                        if continuum_version not in badge_version and badge_version not in continuum_version:
                            issues.append('Version mismatch between API and badge')
                            print("  ⚠️ Version mismatch detected")
                else:
                    issues.append('Version badge not visible or missing')
                    print("  ❌ Version badge missing")
                
                test_result['issues'] = issues
                test_result['status'] = 'healthy' if not issues else 'degraded'
                
            else:
                test_result['status'] = 'error'
                test_result['issues'].append('Version test execution failed')
                print("  ❌ Version test execution failed")
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"Version test error: {str(e)}")
            print(f"  ❌ Version test error: {e}")
        
        return test_result
    
    async def _test_screenshot_functionality(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test screenshot functionality end-to-end"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            # Test bytes mode first (faster)
            bytes_result = await client.js.execute("""
                if (typeof window.ScreenshotUtils === 'undefined') {
                    return {success: false, error: 'ScreenshotUtils not available'};
                }
                
                var testElement = document.querySelector('.version-badge') || document.body;
                
                return window.ScreenshotUtils.takeScreenshot(testElement, {
                    scale: 0.5,
                    source: 'diagnostic_test'
                }).then(function(canvas) {
                    return {
                        success: true,
                        mode: 'bytes',
                        width: canvas.width,
                        height: canvas.height,
                        dataSize: canvas.toDataURL('image/png').length
                    };
                }).catch(function(error) {
                    return {success: false, error: error.message};
                });
            """)
            
            bytes_success = False
            if bytes_result['success']:
                bytes_data = json.loads(bytes_result['result'])
                if bytes_data['success']:
                    bytes_success = True
                    test_result['details']['bytes_mode'] = {
                        'width': bytes_data['width'],
                        'height': bytes_data['height'],
                        'data_size': bytes_data['dataSize']
                    }
                    print(f"  ✅ Bytes mode: {bytes_data['width']}x{bytes_data['height']}")
                else:
                    test_result['issues'].append(f"Bytes mode failed: {bytes_data.get('error')}")
                    print(f"  ❌ Bytes mode failed: {bytes_data.get('error')}")
            else:
                test_result['issues'].append('ScreenshotUtils execution failed')
                print("  ❌ ScreenshotUtils execution failed")
            
            # Test file mode
            file_result = await client.js.execute("""
                if (typeof window.continuum === 'undefined' || !window.continuum.command) {
                    return {success: false, error: 'continuum.command not available'};
                }
                
                try {
                    window.continuum.command.screenshot({
                        selector: 'body',
                        name_prefix: 'diagnostic_test',
                        scale: 0.25,
                        destination: 'file'
                    });
                    
                    return {success: true, mode: 'file'};
                } catch (error) {
                    return {success: false, error: error.message};
                }
            """)
            
            file_success = False
            if file_result['success']:
                file_data = json.loads(file_result['result'])
                if file_data['success']:
                    file_success = True
                    print("  ✅ File mode command sent")
                    
                    # Check for file creation
                    await asyncio.sleep(3)
                    screenshot_dir = Path('.continuum/screenshots')
                    if screenshot_dir.exists():
                        files = list(screenshot_dir.glob('diagnostic_test_*.png'))
                        if files:
                            latest_file = max(files, key=lambda f: f.stat().st_mtime)
                            test_result['details']['file_mode'] = {
                                'file_created': True,
                                'filename': latest_file.name,
                                'file_size': latest_file.stat().st_size
                            }
                            print(f"    ✅ File created: {latest_file.name}")
                        else:
                            test_result['issues'].append('File mode: no file created')
                            print("    ❌ File mode: no file created")
                    else:
                        test_result['issues'].append('Screenshots directory not found')
                        print("    ❌ Screenshots directory not found")
                else:
                    test_result['issues'].append(f"File mode failed: {file_data.get('error')}")
                    print(f"  ❌ File mode failed: {file_data.get('error')}")
            else:
                test_result['issues'].append('File mode execution failed')
                print("  ❌ File mode execution failed")
            
            # Overall status
            if bytes_success and file_success:
                test_result['status'] = 'healthy'
            elif bytes_success or file_success:
                test_result['status'] = 'degraded'
            else:
                test_result['status'] = 'unhealthy'
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"Screenshot test error: {str(e)}")
            print(f"  ❌ Screenshot test error: {e}")
        
        return test_result
    
    async def _test_websocket_communication(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test WebSocket communication"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            result = await client.js.execute(f"""
                var wsTest = {{
                    available: typeof window.ws !== 'undefined',
                    readyState: null,
                    url: null,
                    testSent: false
                }};
                
                if (wsTest.available) {{
                    wsTest.readyState = window.ws.readyState;
                    wsTest.url = window.ws.url;
                    
                    // Test sending a message
                    if (window.ws.readyState === WebSocket.OPEN) {{
                        try {{
                            window.ws.send(JSON.stringify({{
                                type: 'diagnostic_test',
                                timestamp: Date.now(),
                                message: 'WebSocket diagnostic test'
                            }}));
                            wsTest.testSent = true;
                        }} catch (e) {{
                            wsTest.sendError = e.message;
                        }}
                    }}
                }}
                
                return {{
                    success: true,
                    wsTest: wsTest
                }};
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                ws_test = data['wsTest']
                
                test_result['details'] = ws_test
                
                if ws_test['available']:
                    if ws_test['readyState'] == 1:  # OPEN
                        if ws_test['testSent']:
                            test_result['status'] = 'healthy'
                            print("  ✅ WebSocket communication working")
                        else:
                            test_result['status'] = 'degraded'
                            test_result['issues'].append('WebSocket send failed')
                            print("  ⚠️ WebSocket send failed")
                    else:
                        test_result['status'] = 'unhealthy'
                        test_result['issues'].append(f"WebSocket not open (state: {ws_test['readyState']})")
                        print(f"  ❌ WebSocket not open (state: {ws_test['readyState']})")
                else:
                    test_result['status'] = 'unhealthy'
                    test_result['issues'].append('WebSocket not available')
                    print("  ❌ WebSocket not available")
            else:
                test_result['status'] = 'error'
                test_result['issues'].append('WebSocket test execution failed')
                print("  ❌ WebSocket test execution failed")
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"WebSocket test error: {str(e)}")
            print(f"  ❌ WebSocket test error: {e}")
        
        return test_result
    
    async def _test_filesystem_access(self) -> Dict[str, Any]:
        """Test file system access and permissions"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            # Check continuum directory structure
            continuum_dir = Path('.continuum')
            screenshots_dir = Path('.continuum/screenshots')
            
            test_result['details'] = {
                'continuum_dir_exists': continuum_dir.exists(),
                'screenshots_dir_exists': screenshots_dir.exists(),
                'continuum_dir_writable': None,
                'screenshot_files_count': 0
            }
            
            if continuum_dir.exists():
                # Test write permissions
                try:
                    test_file = continuum_dir / 'diagnostic_test.tmp'
                    test_file.write_text('diagnostic test')
                    test_file.unlink()
                    test_result['details']['continuum_dir_writable'] = True
                    print("  ✅ .continuum directory writable")
                except Exception:
                    test_result['details']['continuum_dir_writable'] = False
                    test_result['issues'].append('.continuum directory not writable')
                    print("  ❌ .continuum directory not writable")
            else:
                test_result['issues'].append('.continuum directory missing')
                print("  ❌ .continuum directory missing")
            
            if screenshots_dir.exists():
                screenshot_files = list(screenshots_dir.glob('*.png'))
                test_result['details']['screenshot_files_count'] = len(screenshot_files)
                print(f"  ✅ Screenshots directory: {len(screenshot_files)} files")
            else:
                test_result['issues'].append('Screenshots directory missing')
                print("  ❌ Screenshots directory missing")
            
            # Overall status
            if (test_result['details']['continuum_dir_exists'] and 
                test_result['details']['continuum_dir_writable'] and
                test_result['details']['screenshots_dir_exists']):
                test_result['status'] = 'healthy'
            elif test_result['details']['continuum_dir_exists']:
                test_result['status'] = 'degraded'
            else:
                test_result['status'] = 'unhealthy'
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"Filesystem test error: {str(e)}")
            print(f"  ❌ Filesystem test error: {e}")
        
        return test_result
    
    async def _test_widget_detection(self, client: ContinuumClient) -> Dict[str, Any]:
        """Test detection of key UI widgets"""
        
        test_result = {
            'status': 'unknown',
            'details': {},
            'issues': []
        }
        
        try:
            result = await client.js.execute("""
                var widgets = {
                    version_badge: null,
                    users_agents: null,
                    active_projects: null,
                    status_indicators: null
                };
                
                // Version badge
                var versionBadge = document.querySelector('.version-badge');
                if (versionBadge) {
                    widgets.version_badge = {
                        found: true,
                        visible: versionBadge.offsetWidth > 0 && versionBadge.offsetHeight > 0,
                        text: versionBadge.textContent.trim(),
                        dimensions: {
                            width: versionBadge.offsetWidth,
                            height: versionBadge.offsetHeight
                        }
                    };
                } else {
                    widgets.version_badge = {found: false};
                }
                
                // Users/Agents section
                var userAgentSelectors = ['.users-agents', '[class*="user"]', '[class*="agent"]'];
                for (var selector of userAgentSelectors) {
                    var elements = document.querySelectorAll(selector);
                    if (elements.length > 0) {
                        widgets.users_agents = {
                            found: true,
                            selector: selector,
                            count: elements.length,
                            visible: elements[0].offsetWidth > 0 && elements[0].offsetHeight > 0
                        };
                        break;
                    }
                }
                if (!widgets.users_agents) {
                    widgets.users_agents = {found: false};
                }
                
                // Active projects
                var projectSelectors = ['.active-projects', '[class*="project"]', '.project-list'];
                for (var selector of projectSelectors) {
                    var elements = document.querySelectorAll(selector);
                    if (elements.length > 0) {
                        widgets.active_projects = {
                            found: true,
                            selector: selector,
                            count: elements.length,
                            visible: elements[0].offsetWidth > 0 && elements[0].offsetHeight > 0
                        };
                        break;
                    }
                }
                if (!widgets.active_projects) {
                    widgets.active_projects = {found: false};
                }
                
                // Status indicators
                var statusSelectors = ['.status', '[class*="status"]', '.indicator'];
                for (var selector of statusSelectors) {
                    var elements = document.querySelectorAll(selector);
                    if (elements.length > 0) {
                        widgets.status_indicators = {
                            found: true,
                            selector: selector,
                            count: elements.length,
                            visible: elements[0].offsetWidth > 0 && elements[0].offsetHeight > 0
                        };
                        break;
                    }
                }
                if (!widgets.status_indicators) {
                    widgets.status_indicators = {found: false};
                }
                
                return {
                    success: true,
                    widgets: widgets
                };
            """)
            
            if result['success']:
                data = json.loads(result['result'])
                widgets = data['widgets']
                
                test_result['details'] = widgets
                
                found_widgets = 0
                visible_widgets = 0
                
                for widget_name, widget_data in widgets.items():
                    if widget_data['found']:
                        found_widgets += 1
                        if widget_data.get('visible'):
                            visible_widgets += 1
                            print(f"  ✅ {widget_name}: found and visible")
                        else:
                            print(f"  ⚠️ {widget_name}: found but not visible")
                            test_result['issues'].append(f"{widget_name} not visible")
                    else:
                        print(f"  ❌ {widget_name}: not found")
                        test_result['issues'].append(f"{widget_name} not found")
                
                # Special attention to known broken widgets
                if not widgets['active_projects']['found']:
                    test_result['issues'].append("Active projects widget missing (known issue)")
                
                # Overall status
                if found_widgets >= 3:
                    test_result['status'] = 'healthy'
                elif found_widgets >= 2:
                    test_result['status'] = 'degraded'
                else:
                    test_result['status'] = 'unhealthy'
                
                test_result['details']['summary'] = {
                    'found_widgets': found_widgets,
                    'visible_widgets': visible_widgets,
                    'total_tested': len(widgets)
                }
                
            else:
                test_result['status'] = 'error'
                test_result['issues'].append('Widget detection execution failed')
                print("  ❌ Widget detection execution failed")
                
        except Exception as e:
            test_result['status'] = 'error'
            test_result['issues'].append(f"Widget detection error: {str(e)}")
            print(f"  ❌ Widget detection error: {e}")
        
        return test_result
    
    def _generate_summary(self) -> Dict[str, Any]:
        """Generate diagnostic summary"""
        
        total_tests = len(self.results)
        healthy_tests = sum(1 for r in self.results.values() if r['status'] == 'healthy')
        degraded_tests = sum(1 for r in self.results.values() if r['status'] == 'degraded')
        unhealthy_tests = sum(1 for r in self.results.values() if r['status'] == 'unhealthy')
        error_tests = sum(1 for r in self.results.values() if r['status'] == 'error')
        
        all_issues = []
        for test_result in self.results.values():
            all_issues.extend(test_result.get('issues', []))
        
        # Overall health score
        health_score = (healthy_tests * 100 + degraded_tests * 70) / (total_tests * 100) if total_tests > 0 else 0
        
        if health_score >= 0.9:
            overall_status = 'healthy'
        elif health_score >= 0.7:
            overall_status = 'degraded'
        elif health_score >= 0.5:
            overall_status = 'unhealthy'
        else:
            overall_status = 'critical'
        
        summary = {
            'overall_status': overall_status,
            'health_score': round(health_score, 2),
            'test_counts': {
                'total': total_tests,
                'healthy': healthy_tests,
                'degraded': degraded_tests,
                'unhealthy': unhealthy_tests,
                'error': error_tests
            },
            'total_issues': len(all_issues),
            'critical_issues': [issue for issue in all_issues if 'not available' in issue.lower() or 'missing' in issue.lower()],
            'duration': round(self.end_time - self.start_time, 2) if self.end_time and self.start_time else None
        }
        
        return summary
    
    async def _save_diagnostic_results(self):
        """Save diagnostic results to file"""
        
        try:
            results_dir = Path('.continuum/diagnostics')
            results_dir.mkdir(exist_ok=True)
            
            timestamp = datetime.now().strftime('%Y%m%d_%H%M%S')
            results_file = results_dir / f'diagnostic_results_{timestamp}.json'
            
            full_results = {
                'timestamp': datetime.now().isoformat(),
                'duration': self.end_time - self.start_time if self.end_time and self.start_time else None,
                'summary': self._generate_summary(),
                'results': self.results
            }
            
            with open(results_file, 'w') as f:
                json.dump(full_results, f, indent=2)
            
            print(f"\n💾 Diagnostic results saved: {results_file}")
            
        except Exception as e:
            print(f"⚠️ Failed to save diagnostic results: {e}")
    
    def print_summary(self, summary: Dict[str, Any]):
        """Print diagnostic summary"""
        
        print("\n" + "=" * 50)
        print("🔧 CONTINUUM DIAGNOSTIC SUMMARY")
        print("=" * 50)
        
        status_emoji = {
            'healthy': '✅',
            'degraded': '⚠️',
            'unhealthy': '❌',
            'critical': '🔥'
        }
        
        print(f"{status_emoji.get(summary['overall_status'], '❓')} Overall Status: {summary['overall_status'].upper()}")
        print(f"📊 Health Score: {summary['health_score']:.1%}")
        print(f"⏱️ Duration: {summary['duration']}s")
        
        counts = summary['test_counts']
        print(f"\n📋 Test Results:")
        print(f"  ✅ Healthy: {counts['healthy']}/{counts['total']}")
        print(f"  ⚠️ Degraded: {counts['degraded']}/{counts['total']}")
        print(f"  ❌ Unhealthy: {counts['unhealthy']}/{counts['total']}")
        print(f"  🔥 Error: {counts['error']}/{counts['total']}")
        
        if summary['critical_issues']:
            print(f"\n🔥 Critical Issues ({len(summary['critical_issues'])}):")
            for issue in summary['critical_issues'][:5]:  # Show first 5
                print(f"  • {issue}")
        
        print("\n" + "=" * 50)


# CLI function for easy access
async def run_diagnostics():
    """Run diagnostics from command line"""
    diagnostics = ContinuumDiagnostics()
    results = await diagnostics.run_full_diagnostics()
    diagnostics.print_summary(results['summary'])
    return results


if __name__ == "__main__":
    # Run diagnostics directly
    asyncio.run(run_diagnostics())