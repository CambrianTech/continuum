#!/usr/bin/env node
/**
 * Cursor Movement Demo - Show real cursor movement
 */

const { spawn } = require('child_process');

class CursorMover {
  constructor() {
    this.positions = [
      [100, 100],   // Top-left
      [800, 100],   // Top-right  
      [800, 600],   // Bottom-right
      [100, 600],   // Bottom-left
      [400, 300],   // Center
      [200, 200],   // Upper-left
      [600, 400],   // Lower-right
      [400, 150],   // Top-center
    ];
    this.currentIndex = 0;
  }

  async moveToPosition(x, y) {
    return new Promise((resolve, reject) => {
      console.log(`🖱️ Moving cursor to (${x}, ${y})`);
      
      // Use AppleScript to move cursor on macOS
      const script = `tell application "System Events" to set cursor to {${x}, ${y}}`;
      const process = spawn('osascript', ['-e', script]);
      
      process.on('close', (code) => {
        if (code === 0) {
          resolve();
        } else {
          reject(new Error(`Failed to move cursor: ${code}`));
        }
      });
      
      process.on('error', (error) => {
        reject(error);
      });
    });
  }

  async takeScreenshot(name) {
    return new Promise((resolve, reject) => {
      const filename = `.continuum/cursor-demo-${name}-${Date.now()}.png`;
      console.log(`📸 Taking screenshot: ${filename}`);
      
      const process = spawn('screencapture', ['-C', filename]);
      
      process.on('close', (code) => {
        if (code === 0) {
          console.log(`✅ Screenshot saved: ${filename}`);
          resolve(filename);
        } else {
          reject(new Error(`Screenshot failed: ${code}`));
        }
      });
    });
  }

  async demo() {
    console.log('🎬 Starting Cursor Movement Demo');
    console.log('Watch your screen - the cursor will move in a pattern!');
    
    for (let i = 0; i < this.positions.length; i++) {
      const [x, y] = this.positions[i];
      const positionName = [
        'top-left', 'top-right', 'bottom-right', 'bottom-left', 
        'center', 'upper-left', 'lower-right', 'top-center'
      ][i];
      
      try {
        // Move cursor
        await this.moveToPosition(x, y);
        
        // Wait a moment
        await new Promise(resolve => setTimeout(resolve, 1000));
        
        // Take screenshot
        await this.takeScreenshot(positionName);
        
        // Wait before next movement
        await new Promise(resolve => setTimeout(resolve, 1500));
        
      } catch (error) {
        console.error(`❌ Error at position ${positionName}:`, error.message);
      }
    }
    
    console.log('✨ Cursor movement demo complete!');
    console.log('📂 Check .continuum/ directory for screenshots showing cursor positions');
  }
}

// Run the demo
new CursorMover().demo().catch(console.error);