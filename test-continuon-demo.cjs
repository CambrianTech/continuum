#!/usr/bin/env node
/**
 * Continuon Demo & Screenshot Capture
 * Test the fixed home positioning and capture UI screenshots
 */

const WebSocket = require('ws');
const { spawn } = require('child_process');

class ContinuonDemo {
  constructor() {
    this.ws = null;
    this.isConnected = false;
    this.screenshots = [];
  }

  async connect() {
    return new Promise((resolve, reject) => {
      console.log('🔌 Connecting to Continuum for Continuon demo...');
      
      this.ws = new WebSocket('ws://localhost:5555');
      
      this.ws.on('open', () => {
        console.log('✅ Connected to Continuum');
        this.isConnected = true;
        resolve();
      });
      
      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data);
          if (message.type === 'result') {
            console.log(`📨 AI Response: ${message.data.result.substring(0, 80)}...`);
          }
        } catch (error) {
          // Ignore parsing errors
        }
      });
      
      this.ws.on('error', reject);
    });
  }

  sendMessage(text) {
    if (this.isConnected) {
      console.log(`📤 ${text}`);
      this.ws.send(JSON.stringify({
        type: 'userMessage',
        message: text
      }));
    }
  }

  async takeScreenshot(name, description) {
    return new Promise((resolve, reject) => {
      const timestamp = Date.now();
      const filename = `.continuum/continuon-demo-${name}-${timestamp}.png`;
      console.log(`📸 Taking screenshot: ${description}`);
      
      const process = spawn('screencapture', ['-C', filename]);
      
      process.on('close', (code) => {
        if (code === 0) {
          console.log(`✅ Screenshot saved: ${filename}`);
          this.screenshots.push({ name, filename, description, timestamp });
          resolve(filename);
        } else {
          reject(new Error(`Screenshot failed: ${code}`));
        }
      });
    });
  }

  async runDemo() {
    await this.connect();
    
    console.log('\n🎬 Continuon Demo & Screenshot Session Starting...');
    console.log('🟢 This will demonstrate the Continuon and capture UI screenshots');
    
    // Step 1: Initial state
    this.sendMessage("Welcome to the Continuon demonstration! 🟢 I'm about to show you our AI visual control system in action.");
    await this.wait(3000);
    
    await this.takeScreenshot('01-initial-state', 'Continuum interface with Continuon in home position');
    await this.wait(2000);
    
    // Step 2: Activate Continuon
    this.sendMessage("Now I'll activate the Continuon - watch the green status indicator break free from its home!");
    await this.wait(2000);
    
    this.sendMessage("[CMD:ACTIVATE_CURSOR]");
    await this.wait(3000);
    
    await this.takeScreenshot('02-continuon-activated', 'Continuon activated and ready for movement');
    await this.wait(2000);
    
    // Step 3: Movement demonstration
    this.sendMessage("The Continuon will now demonstrate smooth movement across the screen using Bezier curves!");
    await this.wait(2000);
    
    this.sendMessage("[CMD:MOVE] 200 200 smooth");
    await this.wait(3000);
    
    await this.takeScreenshot('03-continuon-moved', 'Continuon moved to upper-left area with smooth animation');
    await this.wait(2000);
    
    // Step 4: Click demonstration
    this.sendMessage("Now watch the Continuon perform a click with visual feedback!");
    await this.wait(2000);
    
    this.sendMessage("[CMD:CLICK] 400 300 left");
    await this.wait(3000);
    
    await this.takeScreenshot('04-continuon-click', 'Continuon click with white flash animation');
    await this.wait(2000);
    
    // Step 5: Screenshot feedback test
    this.sendMessage("Let's test the screenshot feedback system - you should see a glowing rectangle!");
    await this.wait(2000);
    
    this.sendMessage("[CMD:SCREENSHOT] low 800x600");
    await this.wait(4000);
    
    await this.takeScreenshot('05-screenshot-feedback', 'Screenshot feedback rectangle in action');
    await this.wait(2000);
    
    // Step 6: Movement pattern
    this.sendMessage("The Continuon will now trace a pattern to show its smooth movement capabilities!");
    await this.wait(2000);
    
    const positions = [
      [600, 200, "Moving to top-right"],
      [600, 500, "Moving to bottom-right"], 
      [200, 500, "Moving to bottom-left"],
      [400, 350, "Returning to center"]
    ];
    
    for (let i = 0; i < positions.length; i++) {
      const [x, y, description] = positions[i];
      this.sendMessage(`${description}...`);
      await this.wait(1000);
      
      this.sendMessage(`[CMD:MOVE] ${x} ${y} smooth`);
      await this.wait(3000);
      
      await this.takeScreenshot(`06-pattern-${i+1}`, `${description} - Continuon at (${x}, ${y})`);
      await this.wait(1000);
    }
    
    // Step 7: Return to base
    this.sendMessage("Finally, I'll deactivate the Continuon and watch it return to its home base!");
    await this.wait(2000);
    
    this.sendMessage("[CMD:DEACTIVATE_CURSOR]");
    await this.wait(4000);
    
    await this.takeScreenshot('07-continuon-home', 'Continuon returned to home position in circular container');
    await this.wait(2000);
    
    // Step 8: Summary
    this.sendMessage("🎯 Continuon demonstration complete! The AI visual control system allows you to see exactly where the AI is focused and what actions it's performing. Screenshots captured for documentation!");
    
    console.log('\n✨ Continuon Demo Complete!');
    console.log('📂 Screenshots captured:');
    this.screenshots.forEach(screenshot => {
      console.log(`   📸 ${screenshot.name}: ${screenshot.description}`);
      console.log(`      File: ${screenshot.filename}`);
    });
    
    return this.screenshots;
  }

  wait(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Run the demo and capture screenshots
const demo = new ContinuonDemo();
demo.runDemo().then((screenshots) => {
  console.log(`\n🎉 Demo complete with ${screenshots.length} screenshots captured!`);
  console.log('📝 Ready to update the README with these UI screenshots.');
  setTimeout(() => process.exit(0), 3000);
}).catch(console.error);