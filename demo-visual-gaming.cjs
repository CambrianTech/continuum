#!/usr/bin/env node

/**
 * Visual Gaming Demo
 * Demonstrates AIs playing games through visual feedback (screenshots + Continuon)
 */

const WebSocket = require('ws');

class VisualGamingDemo {
    constructor() {
        this.ws = null;
        this.connected = false;
    }

    async connect() {
        return new Promise((resolve, reject) => {
            console.log('🔗 Connecting to Continuum v0.2.1880...');
            this.ws = new WebSocket('ws://localhost:5555');
            
            this.ws.on('open', () => {
                console.log('✅ Connected to Visual Gaming System');
                this.connected = true;
                resolve();
            });
            
            this.ws.on('error', (error) => {
                console.error('❌ Connection error:', error);
                reject(error);
            });
            
            this.ws.on('message', (data) => {
                try {
                    const message = JSON.parse(data);
                    if (message.type === 'response') {
                        console.log(`🤖 ${message.agent}: ${message.message}`);
                    }
                } catch (e) {
                    // Handle non-JSON messages
                }
            });
        });
    }

    async sendCommand(command) {
        if (!this.connected) {
            throw new Error('Not connected to WebSocket');
        }

        console.log(`📤 Command: ${command}`);
        this.ws.send(JSON.stringify({
            type: 'userMessage',
            message: command
        }));
    }

    async wait(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    async runDemo() {
        try {
            console.log('🎮📸 Visual AI Gaming Demo');
            console.log('=' .repeat(60));
            console.log('AIs learn to play games through visual feedback!');
            console.log('They take screenshots and use Continuon to interact');
            console.log('=' .repeat(60));
            
            await this.connect();
            await this.wait(2000);

            // Demo 1: Start a fast-paced AI vs AI visual game
            console.log('\n🚀 Demo 1: Fast AI vs AI Battle (Screenshots every 500ms)');
            await this.sendCommand('[CMD:START_VISUAL_GAME] tic-tac-toe AI_Alpha AI_Beta 500 low');
            await this.wait(5000);

            // Check status
            console.log('\n📊 Checking game status...');
            await this.sendCommand('[CMD:VISUAL_GAME_STATUS]');
            await this.wait(3000);

            // Demo 2: Change to Academy training speed (super fast)
            console.log('\n🎓 Demo 2: Academy Training Speed (200ms intervals)');
            await this.sendCommand('[CMD:SET_SCREENSHOT_INTERVAL] 200');
            await this.wait(2000);

            // Demo 3: Take high-res screenshot for analysis
            console.log('\n📸📈 Demo 3: High-resolution analysis screenshot');
            await this.sendCommand('[CMD:HIGH_RES_SCREENSHOT] detailed-analysis');
            await this.wait(3000);

            // Demo 4: Human vs AI visual game
            console.log('\n👤🤖 Demo 4: Human vs AI Visual Game');
            await this.sendCommand('[CMD:START_VISUAL_GAME] chess Joel AI_GrandMaster 1000 med');
            await this.wait(4000);

            // Demo 5: Show all active games
            console.log('\n📊 Demo 5: All Active Visual Games');
            await this.sendCommand('[CMD:VISUAL_GAME_STATUS]');
            await this.wait(3000);

            console.log('\n✨ Visual Gaming Features Demonstrated:');
            console.log('📸 Interval Screenshots - AIs "see" game state');
            console.log('🎯 Continuon Control - AIs click and interact');
            console.log('🎓 Academy Integration - Visual learning');
            console.log('⚡ Variable Speed - Fast training, slow analysis');
            console.log('🔍 Multi-Resolution - Low-res training, high-res analysis');
            console.log('👥 Multi-Player - Human vs AI vs AI battles');

            console.log('\n🎯 Try these commands:');
            console.log('[CMD:START_VISUAL_GAME] tic-tac-toe AI_Alpha AI_Beta 1000 low');
            console.log('[CMD:SET_SCREENSHOT_INTERVAL] 500');
            console.log('[CMD:HIGH_RES_SCREENSHOT] analysis');
            console.log('[CMD:VISUAL_GAME_STATUS]');

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
const demo = new VisualGamingDemo();
demo.runDemo().catch(console.error);