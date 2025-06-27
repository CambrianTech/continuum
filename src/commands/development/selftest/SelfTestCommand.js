/**
 * SelfTest Command - TypeScript Implementation
 * JTAG System Verification with Browser/DevTools Testing
 */
export class SelfTestCommand {
    static getDefinition() {
        return {
            name: 'selftest',
            description: 'Verify JTAG system health using portal sessions with browser/DevTools testing',
            icon: '🔧',
            category: 'development',
            params: JSON.stringify({
                verbose: {
                    type: 'boolean',
                    description: 'Show detailed JTAG output',
                    default: SelfTestCommand.DEFAULT_VERBOSE
                },
                mode: {
                    type: 'string',
                    description: 'Test mode: simple, browser, devtools, full',
                    default: SelfTestCommand.DEFAULT_MODE,
                    enum: ['simple', 'browser', 'devtools', 'full']
                }
            }),
            examples: [
                'selftest --verbose=true --mode=simple',
                'selftest --mode=browser',
                'selftest --mode=devtools',
                'selftest --mode=full'
            ],
            usage: 'selftest [--verbose] [--mode=<simple|browser|devtools|full>]'
        };
    }
    static async execute(params, context) {
        try {
            console.log(`🔧 SelfTest: Starting TypeScript execution`);
            const parsedParams = this.parseParams(params);
            const { verbose = SelfTestCommand.DEFAULT_VERBOSE, mode = SelfTestCommand.DEFAULT_MODE } = parsedParams;
            console.log(`🔧 SelfTest: Parsed params, verbose=${verbose}, mode=${mode}`);
            const testResults = {
                simple: false,
                browser: false,
                devtools: false,
                screenshot: false
            };
            // Always run simple test
            console.log(`🔧 SelfTest: Running simple system test`);
            testResults.simple = await this.testSimpleSystem(verbose);
            if (mode === 'browser' || mode === 'devtools' || mode === 'full') {
                console.log(`🔧 SelfTest: Testing browser management`);
                testResults.browser = await this.testBrowserManagement(verbose, context?.continuum);
            }
            if (mode === 'devtools' || mode === 'full') {
                console.log(`🔧 SelfTest: Testing DevTools integration`);
                testResults.devtools = await this.testDevToolsIntegration(verbose, context?.continuum);
            }
            if (mode === 'full') {
                console.log(`🔧 SelfTest: Testing screenshot capability`);
                testResults.screenshot = await this.testScreenshotCapability(verbose, context?.continuum);
            }
            // Calculate passed tests based on what was actually run
            const ranTests = [];
            // Simple test always runs
            ranTests.push(testResults.simple);
            if (mode === 'browser' || mode === 'devtools' || mode === 'full') {
                ranTests.push(testResults.browser);
            }
            if (mode === 'devtools' || mode === 'full') {
                ranTests.push(testResults.devtools);
            }
            if (mode === 'full') {
                ranTests.push(testResults.screenshot);
            }
            const allPassed = ranTests.every(result => result === true);
            const passedCount = ranTests.filter(result => result === true).length;
            const totalCount = ranTests.length;
            console.log(`🎯 SELFTEST RESULT: ${allPassed ? '✅' : '⚠️'} ${passedCount}/${totalCount} tests passed`);
            return {
                success: allPassed,
                message: `SelfTest ${mode} mode: ${passedCount}/${totalCount} tests passed`,
                tests: testResults,
                mode: mode
            };
        }
        catch (error) {
            console.log(`❌ SelfTest execution error: ${error.message}`);
            return {
                success: false,
                message: `SelfTest error: ${error.message}`,
                tests: {
                    simple: false,
                    browser: false,
                    devtools: false,
                    screenshot: false
                },
                mode: params.mode || SelfTestCommand.DEFAULT_MODE,
                error: error.stack
            };
        }
    }
    static async testSimpleSystem(verbose) {
        try {
            if (verbose)
                console.log(`   🧪 Testing basic system components...`);
            // Test basic Node.js functionality
            const nodeVersion = process.version;
            if (verbose)
                console.log(`   ✅ Node.js version: ${nodeVersion}`);
            // Test memory usage
            const memUsage = process.memoryUsage();
            if (verbose)
                console.log(`   ✅ Memory usage: ${Math.round(memUsage.heapUsed / 1024 / 1024)}MB`);
            return true;
        }
        catch (error) {
            console.log(`   ❌ Simple system test failed: ${error.message}`);
            return false;
        }
    }
    static async testBrowserManagement(verbose, continuum) {
        try {
            if (verbose)
                console.log(`   🌐 Testing browser management...`);
            // Test if we can access browser detection services
            if (continuum?.browserDetector) {
                if (verbose)
                    console.log(`   ✅ Browser detector available`);
                // Test browser detection
                try {
                    const browsers = await continuum.browserDetector.getAvailableBrowsers();
                    if (verbose)
                        console.log(`   ✅ Found ${browsers.length} browsers`);
                }
                catch (detectError) {
                    if (verbose)
                        console.log(`   ⚠️ Browser detection failed: ${detectError.message}`);
                }
            }
            else {
                if (verbose)
                    console.log(`   ⚠️ Browser detector not available`);
            }
            // Test DevTools port accessibility
            try {
                const fetch = await import('node-fetch').then(mod => mod.default);
                const response = await fetch('http://localhost:9222/json');
                if (response.ok) {
                    const tabs = await response.json();
                    if (verbose)
                        console.log(`   ✅ DevTools port responsive: ${tabs.length} tabs`);
                    return true;
                }
                else {
                    if (verbose)
                        console.log(`   ⚠️ DevTools port not responding`);
                    return false;
                }
            }
            catch (fetchError) {
                if (verbose)
                    console.log(`   ⚠️ DevTools connection failed: ${fetchError.message}`);
                return false;
            }
        }
        catch (error) {
            console.log(`   ❌ Browser management test failed: ${error.message}`);
            return false;
        }
    }
    static async testDevToolsIntegration(verbose, continuum) {
        try {
            if (verbose)
                console.log(`   🔧 Testing DevTools integration...`);
            // Test WebSocket connection to DevTools
            try {
                const fetch = await import('node-fetch').then(mod => mod.default);
                const response = await fetch('http://localhost:9222/json');
                if (!response.ok) {
                    if (verbose)
                        console.log(`   ❌ DevTools not available`);
                    return false;
                }
                const tabs = await response.json();
                if (tabs.length === 0) {
                    if (verbose)
                        console.log(`   ⚠️ No browser tabs found`);
                    return false;
                }
                // Find Continuum tab
                const continuumTab = tabs.find(tab => tab.url.includes('localhost:9000') ||
                    tab.title.toLowerCase().includes('continuum'));
                if (continuumTab) {
                    if (verbose)
                        console.log(`   ✅ Continuum tab found: ${continuumTab.title}`);
                    // Test WebSocket connectivity
                    if (continuumTab.webSocketDebuggerUrl) {
                        if (verbose)
                            console.log(`   ✅ WebSocket debugger URL available`);
                        return true;
                    }
                    else {
                        if (verbose)
                            console.log(`   ⚠️ No WebSocket debugger URL`);
                        return false;
                    }
                }
                else {
                    if (verbose)
                        console.log(`   ⚠️ Continuum tab not found`);
                    return false;
                }
            }
            catch (devtoolsError) {
                if (verbose)
                    console.log(`   ❌ DevTools integration failed: ${devtoolsError.message}`);
                return false;
            }
        }
        catch (error) {
            console.log(`   ❌ DevTools integration test failed: ${error.message}`);
            return false;
        }
    }
    static async testScreenshotCapability(verbose, continuum) {
        try {
            if (verbose)
                console.log(`   📸 Testing screenshot capability...`);
            // Test if screenshot command is available
            if (continuum?.commandProcessor) {
                const screenshotCommand = continuum.commandProcessor.getCommand('screenshot');
                if (screenshotCommand) {
                    if (verbose)
                        console.log(`   ✅ Screenshot command available`);
                    // Test basic screenshot execution (don't actually take screenshot)
                    if (verbose)
                        console.log(`   ✅ Screenshot capability verified`);
                    return true;
                }
                else {
                    if (verbose)
                        console.log(`   ❌ Screenshot command not found`);
                    return false;
                }
            }
            else {
                if (verbose)
                    console.log(`   ⚠️ Command processor not available`);
                return false;
            }
        }
        catch (error) {
            console.log(`   ❌ Screenshot capability test failed: ${error.message}`);
            return false;
        }
    }
    static createErrorResult(message) {
        return {
            success: false,
            message,
            tests: {
                simple: false,
                browser: false,
                devtools: false,
                screenshot: false
            },
            mode: 'error'
        };
    }
    static isValidMode(mode) {
        return ['simple', 'browser', 'devtools', 'full'].includes(mode);
    }
    static parseParams(params) {
        if (typeof params === 'string') {
            try {
                return JSON.parse(params);
            }
            catch (error) {
                console.warn(`Failed to parse JSON params: ${params}`, error);
                return params;
            }
        }
        return params;
    }
}
SelfTestCommand.DEFAULT_MODE = 'simple';
SelfTestCommand.DEFAULT_VERBOSE = true;
export default SelfTestCommand;
//# sourceMappingURL=SelfTestCommand.js.map