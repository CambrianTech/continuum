/**
 * Clear Command
 * Cleans up debug data for fresh debugging sessions
 * Removes old screenshots, logs, and temporary data
 */

const fs = require('fs');
const path = require('path');

class ClearCommand {
  static getDefinition() {
    return {
      name: 'CLEAR',
      category: 'Core',
      icon: '🧹',
      description: 'Clear debug data for fresh sessions',
      params: '[screenshots|logs|all|cache]',
      examples: [
        '{}',
        '{"params": "screenshots"}',
        '{"params": "logs"}', 
        '{"params": "all"}',
        '{"params": "cache"}'
      ],
      usage: 'Cleans up old debug data to increase signal-to-noise ratio. Use "all" to clear everything, or specify category.'
    };
  }
  
  static async execute(params, continuum) {
    console.log('🧹 CLEAR COMMAND: Starting cleanup with params:', params);
    
    const clearType = params?.trim() || 'all';
    const results = {
      screenshots: 0,
      logs: 0,
      cache: 0,
      errors: []
    };
    
    try {
      const basePath = process.cwd();
      const continuumPath = path.join(basePath, '.continuum');
      
      // Clear screenshots
      if (clearType === 'all' || clearType === 'screenshots') {
        console.log('🧹 CLEAR: Cleaning screenshots...');
        try {
          const screenshotsPath = path.join(continuumPath, 'screenshots');
          if (fs.existsSync(screenshotsPath)) {
            const files = fs.readdirSync(screenshotsPath);
            for (const file of files) {
              if (file.endsWith('.png') || file.endsWith('.jpg') || file.endsWith('.jpeg')) {
                const filePath = path.join(screenshotsPath, file);
                fs.unlinkSync(filePath);
                results.screenshots++;
                console.log(`🧹 CLEAR: Deleted screenshot: ${file}`);
              }
            }
          }
        } catch (error) {
          results.errors.push(`Screenshots: ${error.message}`);
          console.error('🧹 CLEAR: Screenshot cleanup error:', error);
        }
      }
      
      // Clear logs (keep recent entries, remove old ones)
      if (clearType === 'all' || clearType === 'logs') {
        console.log('🧹 CLEAR: Cleaning logs...');
        try {
          const logPath = path.join(basePath, 'continuum.log');
          if (fs.existsSync(logPath)) {
            // Keep only last 50 lines
            const logContent = fs.readFileSync(logPath, 'utf8');
            const lines = logContent.split('\n');
            const recentLines = lines.slice(-50);
            
            // Add fresh session marker
            recentLines.push(`🧹 CLEAR: Fresh debug session started at ${new Date().toISOString()}`);
            
            fs.writeFileSync(logPath, recentLines.join('\n'));
            results.logs = lines.length - recentLines.length;
            console.log(`🧹 CLEAR: Trimmed ${results.logs} old log lines`);
          }
        } catch (error) {
          results.errors.push(`Logs: ${error.message}`);
          console.error('🧹 CLEAR: Log cleanup error:', error);
        }
      }
      
      // Clear cache and temporary data
      if (clearType === 'all' || clearType === 'cache') {
        console.log('🧹 CLEAR: Cleaning cache...');
        try {
          // Clear any cached screenshot data in memory
          if (continuum.screenshotData) {
            const cacheCount = continuum.screenshotData.size;
            continuum.screenshotData.clear();
            results.cache = cacheCount;
            console.log(`🧹 CLEAR: Cleared ${cacheCount} cached screenshots`);
          }
          
          // Clear browser cache via WebSocket
          if (continuum.webSocketServer) {
            continuum.webSocketServer.broadcast({
              type: 'execute_js',
              data: {
                command: `
                  console.log('🧹 CLEAR: Clearing browser cache...');
                  
                  // Clear stored screenshot data
                  delete window.lastScreenshotData;
                  delete window.consoleLogs;
                  delete window.capturedErrors;
                  
                  // Clear any cached canvas elements
                  const cachedCanvases = document.querySelectorAll('canvas[data-screenshot-cache]');
                  cachedCanvases.forEach(canvas => canvas.remove());
                  
                  console.log('🧹 CLEAR: Browser cache cleared');
                `
              }
            });
          }
        } catch (error) {
          results.errors.push(`Cache: ${error.message}`);
          console.error('🧹 CLEAR: Cache cleanup error:', error);
        }
      }
      
      // Summary
      const success = results.errors.length === 0;
      const message = success 
        ? `🧹 CLEAR: Cleanup completed - Screenshots: ${results.screenshots}, Logs: ${results.logs}, Cache: ${results.cache}`
        : `🧹 CLEAR: Cleanup completed with errors - Screenshots: ${results.screenshots}, Logs: ${results.logs}, Cache: ${results.cache}`;
      
      console.log(message);
      
      return {
        success,
        message,
        details: results,
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('🧹 CLEAR: Command failed:', error);
      return {
        success: false,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }
}

module.exports = ClearCommand;