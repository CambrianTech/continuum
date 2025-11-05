/**
 * Simple Browser Launcher - Respects user's default browser choice
 * 
 * PRINCIPLE: Use system default browser unless explicitly overridden
 * NO COMPLEX LOGIC - just launch what the user wants
 */

import { spawn } from 'child_process';

export interface SimpleBrowserConfig {
    url?: string;
    forceDevTools?: boolean;
    forceBrowser?: 'chrome' | 'opera' | 'firefox' | 'safari';
}

export class SimpleBrowserLauncher {
    
    /**
     * Launch browser using system default (respects user choice)
     */
    static async launchDefault(config: SimpleBrowserConfig = {}): Promise<{ pid: number; browser: string }> {
        const url = config.url || 'http://localhost:9000';
        
        console.log(`🌐 Launching default browser for: ${url}`);
        
        // Use system 'open' command - automatically uses user's default browser
        let command = 'open';
        let args = [url];
        
        // Platform-specific open commands
        if (process.platform === 'linux') {
            command = 'xdg-open';
        } else if (process.platform === 'win32') {
            command = 'start';
            args = ['', url]; // Windows start needs empty first arg
        }
        
        try {
            const browserProcess = spawn(command, args, {
                detached: true,
                stdio: 'ignore'
            });
            
            const pid = browserProcess.pid || 0;
            
            console.log(`✅ Browser launched via system default (PID: ${pid})`);
            
            return {
                pid,
                browser: 'system-default'
            };
            
        } catch (error) {
            console.error(`❌ Failed to launch default browser: ${error}`);
            throw error;
        }
    }
    
    /**
     * Launch specific browser with DevTools (for development)
     */
    static async launchWithDevTools(config: SimpleBrowserConfig = {}): Promise<{ pid: number; browser: string; devToolsPort: number }> {
        const url = config.url || 'http://localhost:9000';
        const devToolsPort = 9222;
        
        // For DevTools, we need Chrome or Chromium-based browsers
        const browserPaths = [
            '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
            '/Applications/Opera GX.app/Contents/MacOS/Opera', // Opera GX supports Chrome DevTools Protocol
            '/usr/bin/google-chrome',
            '/usr/bin/chromium-browser'
        ];
        
        let browserPath = null;
        let browserName = 'unknown';
        
        // Find available browser
        const fs = await import('fs');
        for (const path of browserPaths) {
            if (fs.existsSync(path)) {
                browserPath = path;
                if (path.includes('Opera')) browserName = 'opera-gx';
                else if (path.includes('Chrome')) browserName = 'chrome';
                else browserName = 'chromium';
                break;
            }
        }
        
        if (!browserPath) {
            throw new Error('No DevTools-compatible browser found (Chrome/Opera GX/Chromium)');
        }
        
        const args = [
            '--remote-debugging-port=' + devToolsPort,
            '--no-first-run',
            '--disable-default-apps',
            '--user-data-dir=/tmp/continuum-devtools-' + Date.now(),
            url
        ];
        
        try {
            const browserProcess = spawn(browserPath, args, {
                detached: false,
                stdio: 'pipe'
            });
            
            const pid = browserProcess.pid || 0;
            
            console.log(`✅ ${browserName} launched with DevTools (PID: ${pid}, DevTools: ${devToolsPort})`);
            
            return {
                pid,
                browser: browserName,
                devToolsPort
            };
            
        } catch (error) {
            console.error(`❌ Failed to launch ${browserName} with DevTools: ${error}`);
            throw error;
        }
    }
    
    /**
     * Simple launcher - respects user choice
     */
    static async launch(config: SimpleBrowserConfig = {}): Promise<{ pid: number; browser: string; devToolsPort?: number }> {
        // If DevTools explicitly requested, use DevTools launcher
        if (config.forceDevTools) {
            return await this.launchWithDevTools(config);
        }
        
        // Otherwise, use system default (respects user's Opera GX choice)
        const result = await this.launchDefault(config);
        return {
            ...result
        };
    }
}

/**
 * Test function - verify browser launching works
 */
export async function testBrowserLaunching(): Promise<void> {
    console.log('🧪 Testing Simple Browser Launcher...');
    
    try {
        // Test 1: Default browser (should be Opera GX)
        console.log('\n1. Testing default browser launch...');
        const defaultResult = await SimpleBrowserLauncher.launchDefault({
            url: 'http://localhost:9000'
        });
        console.log(`   Result: ${JSON.stringify(defaultResult)}`);
        
        // Test 2: DevTools browser
        console.log('\n2. Testing DevTools browser launch...');
        const devToolsResult = await SimpleBrowserLauncher.launchWithDevTools({
            url: 'http://localhost:9000'
        });
        console.log(`   Result: ${JSON.stringify(devToolsResult)}`);
        
        console.log('\n✅ Browser launcher tests completed');
        
    } catch (error) {
        console.error('❌ Browser launcher test failed:', error);
        throw error;
    }
}