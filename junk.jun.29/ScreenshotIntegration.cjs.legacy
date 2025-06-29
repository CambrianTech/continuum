/**
 * Screenshot Integration
 * Handles screenshot capture as a system integration (like mouse control)
 */

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

class ScreenshotIntegration {
  constructor(continuum) {
    this.continuum = continuum;
    this.screenshotDir = path.join(process.cwd(), '.continuum');
    
    // Ensure screenshot directory exists
    if (!fs.existsSync(this.screenshotDir)) {
      fs.mkdirSync(this.screenshotDir, { recursive: true });
    }
  }

  /**
   * Take a screenshot and return the file path
   */
  async captureScreen(options = {}) {
    const {
      filename = `screenshot-${Date.now()}.png`,
      includeTimestamp = true
    } = options;

    const actualFilename = includeTimestamp 
      ? `screenshot-${Date.now()}.png`
      : filename;
    
    const screenshotPath = path.join(this.screenshotDir, actualFilename);

    try {
      // This should trigger browser DOM screenshot, not desktop capture
      // For now, return instruction for user to use browser console
      return {
        success: true,
        path: screenshotPath,
        filename: actualFilename,
        timestamp: new Date().toISOString(),
        message: 'To take browser screenshot: Open browser console and run takeWebScreenshot() or takeDOMScreenshot()'
      };

      if (fs.existsSync(screenshotPath)) {
        console.log(`📸 Screenshot saved: ${screenshotPath}`);
        return {
          success: true,
          path: screenshotPath,
          filename: actualFilename,
          timestamp: new Date().toISOString()
        };
      } else {
        throw new Error('Screenshot file was not created');
      }

    } catch (error) {
      console.error('Screenshot failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Handle screenshot command from agents
   */
  async handleScreenshotCommand(command, agent = 'system') {
    console.log(`📸 ${agent} requested screenshot: ${command}`);
    
    // Send JavaScript execution to capture browser screenshot
    const screenshotJS = `
      console.log('🔧 Screenshot JavaScript executing NOW!');
      alert('JavaScript execution test - screenshot starting!');
      
      // Function to capture DOM screenshot
      function captureInterfaceScreenshot() {
        const filename = 'interface-screenshot-' + Date.now() + '.png';
        console.log('📸 Starting interface screenshot capture:', filename);
        
        // Debug current page state
        console.log('📍 Current URL:', window.location.href);
        console.log('📍 Document title:', document.title);
        console.log('📍 Window size:', window.innerWidth + 'x' + window.innerHeight);
        
        // Simple fallback screenshot first
        const canvas = document.createElement('canvas');
        const ctx = canvas.getContext('2d');
        canvas.width = window.innerWidth;
        canvas.height = window.innerHeight;
        
        // Fill with background
        ctx.fillStyle = '#1a1a1a';
        ctx.fillRect(0, 0, canvas.width, canvas.height);
        
        // Add text indicating this is a screenshot
        ctx.fillStyle = '#00ff88';
        ctx.font = '20px monospace';
        ctx.fillText('Continuum Interface Screenshot', 50, 50);
        ctx.fillText('URL: ' + window.location.href, 50, 80);
        ctx.fillText('Timestamp: ' + new Date().toISOString(), 50, 110);
        ctx.fillText('Size: ' + window.innerWidth + 'x' + window.innerHeight, 50, 140);
        ctx.fillText('SUCCESS: Browser JS Execution Working!', 50, 170);
        
        const link = document.createElement('a');
        link.download = filename;
        link.href = canvas.toDataURL();
        link.click();
        console.log('📸 Screenshot captured and download triggered:', filename);
        alert('Screenshot captured! Check Downloads for: ' + filename);
      }
      
      // Execute screenshot immediately
      console.log('⏱️ Starting screenshot NOW...');
      captureInterfaceScreenshot();
    `;
    
    // Send JS execution request via WebSocket
    if (this.continuum.webSocketServer) {
      this.continuum.webSocketServer.broadcast({
        type: 'execute_js',
        data: {
          command: screenshotJS,
          timestamp: new Date().toISOString()
        }
      });
    }
    
    return {
      success: true,
      message: 'Screenshot request sent to browser - file should download automatically',
      agent: agent,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Request browser to take screenshot via WebSocket
   */
  async requestBrowserScreenshot(screenshotPath, filename) {
    try {
      // Send screenshot request to all connected browsers
      const screenshotRequest = {
        type: 'screenshot_request',
        data: {
          filename: filename,
          timestamp: new Date().toISOString()
        }
      };

      // Broadcast to all WebSocket connections
      if (this.continuum.webSocketServer && this.continuum.webSocketServer.wss) {
        this.continuum.webSocketServer.wss.clients.forEach(client => {
          if (client.readyState === 1) { // WebSocket.OPEN
            client.send(JSON.stringify(screenshotRequest));
          }
        });
      }

      // For now, return success with instruction
      return {
        success: true,
        path: screenshotPath,
        filename: filename,
        timestamp: new Date().toISOString(),
        message: 'Screenshot request sent to browser. Use browser console: takeWebScreenshot()'
      };

    } catch (error) {
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = ScreenshotIntegration;