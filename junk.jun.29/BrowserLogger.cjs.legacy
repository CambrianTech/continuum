/**
 * Browser Logger - Logs browser console messages and errors to organized files
 */

const fs = require('fs').promises;
const path = require('path');

class BrowserLogger {
  constructor(continuum) {
    this.continuum = continuum;
    this.logDir = path.join(process.cwd(), '.continuum', 'logs', 'browser');
    this.initializeLogDirectory();
  }

  async initializeLogDirectory() {
    try {
      await fs.mkdir(this.logDir, { recursive: true });
      console.log(`📁 Browser logs directory initialized: ${this.logDir}`);
    } catch (error) {
      console.error('Failed to create browser logs directory:', error);
    }
  }

  getLogFileName(type, date = new Date()) {
    const dateStr = date.toISOString().split('T')[0]; // YYYY-MM-DD
    switch (type) {
      case 'console':
        return `browser-console-${dateStr}.log`;
      case 'errors':
        return `browser-errors-${dateStr}.log`;
      case 'performance':
        return `browser-performance-${dateStr}.log`;
      case 'interactions':
        return `browser-interactions-${dateStr}.log`;
      default:
        return `browser-misc-${dateStr}.log`;
    }
  }

  async writeLog(type, data) {
    try {
      const fileName = this.getLogFileName(type);
      const filePath = path.join(this.logDir, fileName);
      
      const timestamp = new Date().toISOString();
      const logEntry = {
        timestamp,
        type,
        data,
        sessionId: data.sessionId || 'unknown',
        tabId: data.tabId || 'unknown'
      };

      const logLine = JSON.stringify(logEntry) + '\n';
      await fs.appendFile(filePath, logLine);
      
      // Also maintain a daily summary
      await this.updateDailySummary(type, logEntry);
      
    } catch (error) {
      console.error(`Failed to write ${type} log:`, error);
    }
  }

  async updateDailySummary(type, logEntry) {
    try {
      const summaryFile = path.join(this.logDir, 'daily-summary.json');
      let summary = {};
      
      try {
        const existing = await fs.readFile(summaryFile, 'utf8');
        summary = JSON.parse(existing);
      } catch (error) {
        // File doesn't exist, start fresh
      }

      const date = new Date().toISOString().split('T')[0];
      if (!summary[date]) {
        summary[date] = {
          console_logs: 0,
          errors: 0,
          performance_logs: 0,
          interactions: 0,
          first_log: logEntry.timestamp,
          last_log: logEntry.timestamp,
          sessions: [],
          tabs: []
        };
      }

      const dayData = summary[date];
      dayData[`${type}_logs`] = (dayData[`${type}_logs`] || 0) + 1;
      dayData.last_log = logEntry.timestamp;
      
      // Add to arrays if not already present
      if (!dayData.sessions.includes(logEntry.sessionId)) {
        dayData.sessions.push(logEntry.sessionId);
      }
      if (!dayData.tabs.includes(logEntry.tabId)) {
        dayData.tabs.push(logEntry.tabId);
      }

      await fs.writeFile(summaryFile, JSON.stringify(summary, null, 2));
      
    } catch (error) {
      console.error('Failed to update daily summary:', error);
    }
  }

  async logConsoleMessage(level, message, metadata = {}) {
    console.log(`🖥️ Browser [${level}]:`, message);
    
    await this.writeLog('console', {
      level,
      message,
      ...metadata
    });
  }

  async logJavaScriptError(error, metadata = {}) {
    console.error('🚨 Browser JS Error:', error);
    
    await this.writeLog('errors', {
      error: typeof error === 'string' ? error : error.message || error.toString(),
      stack: error.stack || 'No stack trace',
      ...metadata
    });
  }

  async logPerformanceMetric(metric, value, metadata = {}) {
    console.log(`⚡ Browser Performance [${metric}]:`, value);
    
    await this.writeLog('performance', {
      metric,
      value,
      ...metadata
    });
  }

  async logUserInteraction(action, target, metadata = {}) {
    console.log(`👆 Browser Interaction [${action}]:`, target);
    
    await this.writeLog('interactions', {
      action,
      target,
      ...metadata
    });
  }

  async getRecentLogs(type = 'console', hours = 24) {
    try {
      const fileName = this.getLogFileName(type);
      const filePath = path.join(this.logDir, fileName);
      
      const content = await fs.readFile(filePath, 'utf8');
      const lines = content.trim().split('\n').filter(line => line);
      
      const cutoffTime = new Date(Date.now() - (hours * 60 * 60 * 1000));
      
      return lines
        .map(line => JSON.parse(line))
        .filter(entry => new Date(entry.timestamp) > cutoffTime)
        .sort((a, b) => new Date(b.timestamp) - new Date(a.timestamp));
        
    } catch (error) {
      console.error(`Failed to read ${type} logs:`, error);
      return [];
    }
  }

  async getLogStats() {
    try {
      const summaryFile = path.join(this.logDir, 'daily-summary.json');
      const content = await fs.readFile(summaryFile, 'utf8');
      const summary = JSON.parse(content);
      
      const today = new Date().toISOString().split('T')[0];
      const yesterday = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString().split('T')[0];
      
      return {
        today: summary[today] || {},
        yesterday: summary[yesterday] || {},
        total_days: Object.keys(summary).length,
        date_range: {
          first: Math.min(...Object.keys(summary)),
          last: Math.max(...Object.keys(summary))
        }
      };
      
    } catch (error) {
      console.error('Failed to get log stats:', error);
      return { today: {}, yesterday: {}, total_days: 0 };
    }
  }

  async cleanupOldLogs(daysToKeep = 30) {
    try {
      const files = await fs.readdir(this.logDir);
      const cutoffDate = new Date(Date.now() - (daysToKeep * 24 * 60 * 60 * 1000));
      
      let deletedCount = 0;
      for (const file of files) {
        if (file.includes('-') && file.endsWith('.log')) {
          const dateMatch = file.match(/(\d{4}-\d{2}-\d{2})/);
          if (dateMatch) {
            const fileDate = new Date(dateMatch[1]);
            if (fileDate < cutoffDate) {
              await fs.unlink(path.join(this.logDir, file));
              deletedCount++;
            }
          }
        }
      }
      
      console.log(`🧹 Cleaned up ${deletedCount} old browser log files`);
      return deletedCount;
      
    } catch (error) {
      console.error('Failed to cleanup old logs:', error);
      return 0;
    }
  }
}

module.exports = BrowserLogger;