#!/usr/bin/env node

/**
 * Continuum Web Browser Integration Demo
 * Demonstrates the embedded web browser functionality within the Continuum console
 */

const WebSocket = require('ws');
const { exec } = require('child_process');

class WebBrowserDemo {
    constructor() {
        this.ws = null;
        this.connected = false;
        this.screenshotCount = 0;
    }

    async connect() {
        return new Promise((resolve, reject) => {
            console.log('🔗 Connecting to Continuum WebSocket...');
            this.ws = new WebSocket('ws://localhost:5555');
            
            this.ws.on('open', () => {
                console.log('✅ Connected to Continuum');
                this.connected = true;
                resolve();
            });
            
            this.ws.on('error', (error) => {
                console.error('❌ WebSocket error:', error);
                reject(error);
            });
            
            this.ws.on('message', (data) => {
                try {
                    const message = JSON.parse(data);
                    if (message.type === 'systemMessage') {
                        console.log(`📝 System: ${message.message}`);
                    }
                } catch (e) {
                    // Ignore non-JSON messages
                }
            });
        });
    }

    async sendCommand(command) {
        if (!this.connected) {
            throw new Error('Not connected to WebSocket');
        }

        console.log(`📤 Sending: ${command}`);
        this.ws.send(JSON.stringify({
            type: 'userMessage',
            message: command
        }));
    }

    async takeScreenshot(description) {
        const timestamp = Date.now();
        const filename = `web-browser-demo-${String(this.screenshotCount + 1).padStart(2, '0')}-${description}-${timestamp}.png`;
        const filepath = `/Users/joel/Development/ideem/vHSM/externals/continuum/.continuum/${filename}`;
        
        return new Promise((resolve) => {
            exec(`screencapture -C "${filepath}"`, (error, stdout, stderr) => {
                if (error) {
                    console.error(`❌ Screenshot error: ${error}`);
                } else {
                    console.log(`📸 Screenshot saved: ${filename}`);
                    this.screenshotCount++;
                }
                resolve(filepath);
            });
        });
    }

    async wait(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async runDemo() {
        try {
            console.log('🌐 Starting Continuum Web Browser Integration Demo');
            console.log('=' .repeat(60));
            
            // Connect to WebSocket
            await this.connect();
            await this.wait(2000);

            // 1. Initial state screenshot
            console.log('\n📸 Step 1: Taking initial state screenshot');
            await this.takeScreenshot('initial-state');
            await this.wait(2000);

            // 2. Activate the web browser via command
            console.log('\n🌐 Step 2: Activating web browser interface');
            await this.sendCommand('[CMD:ACTIVATE_WEB_BROWSER]');
            await this.wait(3000);
            await this.takeScreenshot('web-browser-activated');

            // 3. Navigate to a test website
            console.log('\n🌍 Step 3: Navigating to test website');
            await this.sendCommand('[CMD:WEB_NAVIGATE] https://example.com');
            await this.wait(4000);
            await this.takeScreenshot('web-page-loaded');

            // 4. Activate Continuon for web interaction
            console.log('\n🟢 Step 4: Activating Continuon for web interaction');
            await this.sendCommand('[CMD:ACTIVATE_CURSOR]');
            await this.wait(2000);
            await this.takeScreenshot('continuon-in-web-browser');

            // 5. Move Continuon within web browser
            console.log('\n🎯 Step 5: Moving Continuon within web browser');
            await this.sendCommand('[CMD:MOVE] 640 360 smooth');
            await this.wait(2000);
            await this.takeScreenshot('continuon-moved-in-browser');

            // 6. Click on web content
            console.log('\n🖱️ Step 6: Demonstrating web interaction');
            await this.sendCommand('[CMD:CLICK] 640 400 left');
            await this.wait(2000);
            await this.takeScreenshot('web-interaction-complete');

            // 7. Return to chat view
            console.log('\n💬 Step 7: Returning to chat view');
            await this.sendCommand('[CMD:DEACTIVATE_WEB_BROWSER]');
            await this.wait(2000);
            await this.takeScreenshot('back-to-chat');

            console.log('\n✅ Web Browser Demo Complete!');
            console.log(`📸 Generated ${this.screenshotCount} screenshots`);
            console.log('🎯 Screenshots saved to .continuum/ directory');
            
        } catch (error) {
            console.error('❌ Demo failed:', error);
        } finally {
            if (this.ws) {
                this.ws.close();
            }
            process.exit(0);
        }
    }
}

// Run the demo
const demo = new WebBrowserDemo();
demo.runDemo().catch(console.error);