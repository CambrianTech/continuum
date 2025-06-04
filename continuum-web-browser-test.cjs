#!/usr/bin/env node

/**
 * Test Continuum Web Browser Integration via Real WebSocket Connection
 * This tests the actual Continuum interface and web browser functionality
 */

const WebSocket = require('ws');

class ContinuumWebBrowserTest {
    constructor() {
        this.ws = null;
        this.connected = false;
    }

    async connect() {
        return new Promise((resolve, reject) => {
            console.log('🔗 Connecting to Continuum v0.2.1871...');
            this.ws = new WebSocket('ws://localhost:5555');
            
            this.ws.on('open', () => {
                console.log('✅ Connected to Continuum web interface');
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
                    } else if (message.type === 'aiResponse') {
                        console.log(`🤖 AI: ${message.message}`);
                    }
                } catch (e) {
                    // Handle non-JSON messages
                    console.log(`📨 Raw: ${data}`);
                }
            });
        });
    }

    async sendMessage(message) {
        if (!this.connected) {
            throw new Error('Not connected to WebSocket');
        }

        console.log(`📤 User: ${message}`);
        this.ws.send(JSON.stringify({
            type: 'userMessage',
            message: message
        }));
    }

    async wait(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async testWebBrowser() {
        try {
            console.log('🌐 Testing Continuum Web Browser Integration');
            console.log('=' .repeat(60));
            
            await this.connect();
            await this.wait(2000);

            // Test 1: Activate web browser
            console.log('\n🌐 Test 1: Activating web browser...');
            await this.sendMessage('[CMD:ACTIVATE_WEB_BROWSER]');
            await this.wait(3000);

            // Test 2: Navigate to a website
            console.log('\n🌍 Test 2: Navigating to example.com...');
            await this.sendMessage('[CMD:WEB_NAVIGATE] https://example.com');
            await this.wait(4000);

            // Test 3: Activate Continuon
            console.log('\n🟢 Test 3: Activating Continuon...');
            await this.sendMessage('[CMD:ACTIVATE_CURSOR]');
            await this.wait(2000);

            // Test 4: Move Continuon
            console.log('\n🎯 Test 4: Moving Continuon...');
            await this.sendMessage('[CMD:MOVE] 640 360 smooth');
            await this.wait(2000);

            // Test 5: Take screenshot
            console.log('\n📸 Test 5: Taking screenshot...');
            await this.sendMessage('[CMD:SCREENSHOT] web-browser-test');
            await this.wait(3000);

            // Test 6: Navigate to another site
            console.log('\n🔗 Test 6: Navigate to GitHub...');
            await this.sendMessage('[CMD:WEB_NAVIGATE] https://github.com');
            await this.wait(4000);

            // Test 7: Return to chat
            console.log('\n💬 Test 7: Returning to chat...');
            await this.sendMessage('[CMD:DEACTIVATE_WEB_BROWSER]');
            await this.wait(2000);

            console.log('\n✅ Web Browser Integration Test Complete!');
            
        } catch (error) {
            console.error('❌ Test failed:', error);
        } finally {
            if (this.ws) {
                this.ws.close();
            }
            process.exit(0);
        }
    }
}

// Run the test
const test = new ContinuumWebBrowserTest();
test.testWebBrowser().catch(console.error);