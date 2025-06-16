/**
 * Screenshot Service
 * Handles browser screenshot capture via html2canvas and WebSocket
 * Separated from UI generation for proper separation of concerns
 */

const fs = require('fs');
const path = require('path');

class ScreenshotService {
  constructor(continuum) {
    this.continuum = continuum;
    this.screenshotDir = path.join(process.cwd(), '.continuum', 'screenshots');
    
    // Ensure screenshot directory exists
    if (!fs.existsSync(this.screenshotDir)) {
      fs.mkdirSync(this.screenshotDir, { recursive: true });
    }
  }

  /**
   * Save browser screenshot data received from client
   */
  async saveBrowserScreenshot(base64Data, filename) {
    try {
      console.log(`📸 DEBUG: ScreenshotService.saveBrowserScreenshot called`);
      console.log(`📸 DEBUG: filename=${filename}, screenshotDir=${this.screenshotDir}`);
      console.log(`📸 DEBUG: base64Data length=${base64Data ? base64Data.length : 'null'}`);
      
      const actualFilename = filename || `screenshot-${Date.now()}.png`;
      const screenshotPath = path.join(this.screenshotDir, actualFilename);
      
      console.log(`📸 DEBUG: Full screenshot path: ${screenshotPath}`);
      
      // Remove data URL prefix if present
      const base64Image = base64Data.replace(/^data:image\/png;base64,/, '');
      
      console.log(`📸 DEBUG: Cleaned base64 length: ${base64Image.length}`);
      
      // Save base64 data as PNG file
      fs.writeFileSync(screenshotPath, base64Image, 'base64');
      
      console.log(`📸 Screenshot saved: ${screenshotPath}`);
      
      // Use centralized SHARE command
      const ShareCommand = require('../commands/core/ShareCommand.cjs');
      await ShareCommand.share(screenshotPath, 'user');
      
      return {
        success: true,
        path: screenshotPath,
        filename: actualFilename,
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('Screenshot save failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
  

  /**
   * Take screenshot and provide feedback to user
   */
  async takeScreenshotWithFeedback(ws, sessionId) {
    const result = await this.takeScreenshot();
    
    if (result.success) {
      // Send success message through WebSocket
      if (ws && ws.readyState === 1) {
        ws.send(JSON.stringify({
          type: 'screenshot_result',
          data: {
            success: true,
            message: `📸 Screenshot captured: ${result.filename}`,
            path: result.path,
            timestamp: result.timestamp
          }
        }));
      }
      return result;
    } else {
      // Send error message through WebSocket
      if (ws && ws.readyState === 1) {
        ws.send(JSON.stringify({
          type: 'screenshot_result',
          data: {
            success: false,
            message: `❌ Screenshot failed: ${result.error}`,
            timestamp: result.timestamp
          }
        }));
      }
      return result;
    }
  }

  /**
   * List recent screenshots
   */
  getRecentScreenshots(limit = 10) {
    try {
      const files = fs.readdirSync(this.screenshotDir)
        .filter(file => file.startsWith('screenshot-') && file.endsWith('.png'))
        .map(file => ({
          filename: file,
          path: path.join(this.screenshotDir, file),
          timestamp: fs.statSync(path.join(this.screenshotDir, file)).mtime
        }))
        .sort((a, b) => b.timestamp - a.timestamp)
        .slice(0, limit);

      return files;
    } catch (error) {
      console.error('Failed to list screenshots:', error);
      return [];
    }
  }

  /**
   * Clean up old screenshots (keep last N)
   */
  cleanupOldScreenshots(keepCount = 20) {
    try {
      const screenshots = this.getRecentScreenshots(1000); // Get all
      
      if (screenshots.length > keepCount) {
        const toDelete = screenshots.slice(keepCount);
        toDelete.forEach(screenshot => {
          try {
            fs.unlinkSync(screenshot.path);
            console.log(`🗑️ Deleted old screenshot: ${screenshot.filename}`);
          } catch (error) {
            console.error(`Failed to delete ${screenshot.filename}:`, error);
          }
        });
      }
    } catch (error) {
      console.error('Failed to cleanup screenshots:', error);
    }
  }
}

module.exports = ScreenshotService;