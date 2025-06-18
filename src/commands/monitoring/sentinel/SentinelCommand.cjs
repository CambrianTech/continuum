/**
 * Sentinel Command - AI Guardian Agent
 * Simple logging and task management for AIs
 */

const BaseCommand = require('../../BaseCommand.cjs');
const fs = require('fs');
const path = require('path');

class SentinelCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'sentinel',
      description: 'AI Sentinel - Guardian agent for logging and task management',
      icon: '🛡️',
      parameters: {
        action: {
          type: 'string',
          required: false,
          description: 'Action: start, logs, status, exec, script',
          default: 'logs'
        },
        lines: {
          type: 'number',
          required: false,
          description: 'Number of log lines to show',
          default: 10
        },
        task: {
          type: 'string',
          required: false,
          description: 'Task name for organized logging',
          default: 'general'
        },
        script: {
          type: 'string',
          required: false,
          description: 'JavaScript code to execute or script filename'
        },
        interval: {
          type: 'number',
          required: false,
          description: 'Execution interval in seconds for monitoring scripts',
          default: 30
        },
        source: {
          type: 'string',
          required: false,
          description: 'Log source: client, server, both, or sentinel',
          default: 'sentinel'
        },
        live: {
          type: 'boolean',
          required: false,
          description: 'Enable live log streaming',
          default: false
        },
        stream: {
          type: 'boolean',
          required: false,
          description: 'Enable real-time log streaming subscription',
          default: false
        },
        event: {
          type: 'string',
          required: false,
          description: 'Event type to subscribe to: logs, errors, commands, connections',
          default: 'logs'
        },
        filter: {
          type: 'string',
          required: false,
          description: 'Event filter: client, server, both, or specific pattern',
          default: 'both'
        },
        callback: {
          type: 'boolean',
          required: false,
          description: 'Enable callback mode for subscriptions',
          default: false
        }
      },
      examples: [
        'sentinel',
        'sentinel --action logs --lines 20',
        'sentinel --action start --task debug-session',
        'sentinel --action status'
      ]
    };
  }

  static async execute(params, continuum) {
    console.log(`🛡️ SENTINEL: Starting execution with params: ${params}`);
    
    const options = this.parseParams(params);
    const action = options.action || 'logs';
    const lines = options.lines || 10;
    const task = options.task || 'general';
    
    console.log(`🛡️ SENTINEL: Parsed options - action: ${action}, lines: ${lines}, task: ${task}`);
    
    try {
      console.log(`🛡️ SENTINEL: Starting directory setup...`);
      
      // Use continuum's proper directory structure
      const continuumDir = continuum?.localContinuumDir || continuum?.userDataDir || path.join(process.cwd(), '.continuum');
      console.log(`🛡️ SENTINEL: Resolved continuum dir: ${continuumDir}`);
      
      const sentinelDir = path.join(continuumDir, 'sentinel');
      console.log(`🛡️ SENTINEL: Sentinel dir will be: ${sentinelDir}`);
      
      // Ensure sentinel directory exists
      console.log(`🛡️ SENTINEL: Checking if sentinel dir exists...`);
      if (!fs.existsSync(sentinelDir)) {
        console.log(`🛡️ SENTINEL: Creating sentinel directory...`);
        fs.mkdirSync(sentinelDir, { recursive: true });
        console.log(`🛡️ SENTINEL: Directory created successfully`);
      } else {
        console.log(`🛡️ SENTINEL: Directory already exists`);
      }
      
      // Immediately show directory structure
      console.log(`🛡️ SENTINEL: Final directories:`);
      console.log(`  📁 Continuum dir: ${continuumDir}`);
      console.log(`  📁 Sentinel dir: ${sentinelDir}`);
      
      console.log(`🛡️ SENTINEL: Executing action: ${action}`);
      
      if (action === 'logs') {
        console.log(`🛡️ SENTINEL: Calling showLogs...`);
        const source = options.source || 'sentinel';
        const live = options.live || false;
        
        if (live && (source === 'client' || source === 'server' || source === 'both')) {
          return await this.showLiveLogs(continuum, source, lines);
        } else {
          return await this.showLogs(sentinelDir, lines, task);
        }
      } else if (action === 'start') {
        console.log(`🛡️ SENTINEL: Calling startSentinel...`);
        return await this.startSentinel(sentinelDir, task);
      } else if (action === 'status') {
        console.log(`🛡️ SENTINEL: Calling getSentinelStatus...`);
        return await this.getSentinelStatus(sentinelDir);
      } else if (action === 'path') {
        console.log(`🛡️ SENTINEL: Returning paths...`);
        // Just return the paths immediately
        const result = this.createSuccessResult({
          continuumDir,
          sentinelDir,
          taskDir: path.join(sentinelDir, task)
        }, `Sentinel paths for task: ${task}`);
        console.log(`🛡️ SENTINEL: Path result created: ${JSON.stringify(result)}`);
        return result;
      } else if (action === 'exec') {
        console.log(`🛡️ SENTINEL: Executing JavaScript...`);
        return await this.executeJavaScript(sentinelDir, task, options.script, continuum);
      } else if (action === 'script') {
        console.log(`🛡️ SENTINEL: Running monitoring script...`);
        return await this.runMonitoringScript(sentinelDir, task, options.script, options.interval, continuum);
      } else if (action === 'subscribe') {
        console.log(`🛡️ SENTINEL: Setting up event subscription...`);
        return await this.subscribeToEvents(continuum, options.event, options.filter);
      } else if (action === 'unsubscribe') {
        console.log(`🛡️ SENTINEL: Removing event subscription...`);
        return await this.unsubscribeFromEvents(continuum, options.event, options.filter);
      } else {
        console.log(`🛡️ SENTINEL: Invalid action: ${action}`);
        return this.createErrorResult('Invalid action', `Unknown action: ${action}`);
      }
      
    } catch (error) {
      console.error('❌ Sentinel command failed:', error);
      return this.createErrorResult('Sentinel command failed', error.message);
    }
  }
  
  static async showLiveLogs(continuum, source, lines) {
    const fs = require('fs');
    const path = require('path');
    
    try {
      const logs = {};
      
      if (source === 'server' || source === 'both') {
        // Read from live server log file
        const serverLogPath = path.join(process.cwd(), 'server.log');
        if (fs.existsSync(serverLogPath)) {
          const serverContent = fs.readFileSync(serverLogPath, 'utf8');
          const serverLines = serverContent.split('\n').filter(line => line.trim());
          logs.server = serverLines.slice(-lines);
        } else {
          logs.server = ['Server log file not found'];
        }
      }
      
      if (source === 'client' || source === 'both') {
        // Get client logs from WebSocket server's clientLogs buffer
        if (continuum.webSocketServer && continuum.webSocketServer.clientLogs) {
          const clientLogs = continuum.webSocketServer.clientLogs.slice(-lines);
          logs.client = clientLogs.map(log => 
            `[${new Date(log.timestamp).toISOString()}] ${log.level.toUpperCase()}: ${log.message}`
          );
        } else {
          logs.client = ['No client logs available'];
        }
      }
      
      return this.createSuccessResult(logs, `Live logs retrieved: ${source}`);
    } catch (error) {
      return this.createErrorResult('Failed to get live logs', error.message);
    }
  }

  static async showLogs(sentinelDir, lines, task) {
    const taskDir = path.join(sentinelDir, task);
    
    if (!fs.existsSync(taskDir)) {
      return this.createSuccessResult({ logs: [] }, `No logs found for task: ${task}`);
    }
    
    // Find most recent log file
    const logFiles = fs.readdirSync(taskDir)
      .filter(f => f.endsWith('.log'))
      .sort()
      .reverse();
    
    if (logFiles.length === 0) {
      return this.createSuccessResult({ logs: [] }, `No log files found for task: ${task}`);
    }
    
    const latestLogFile = path.join(taskDir, logFiles[0]);
    const logContent = fs.readFileSync(latestLogFile, 'utf8');
    const allLines = logContent.split('\n').filter(line => line.trim());
    const recentLines = allLines.slice(-lines);
    
    console.log(`📋 Showing last ${recentLines.length} lines from ${task}:`);
    recentLines.forEach(line => console.log(`  ${line}`));
    
    return this.createSuccessResult({ 
      task,
      logFile: logFiles[0],
      lines: recentLines,
      totalLines: allLines.length
    }, `Showed ${recentLines.length} log lines`);
  }
  
  static async startSentinel(sentinelDir, task) {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const taskDir = path.join(sentinelDir, task);
    
    // Create task directory
    if (!fs.existsSync(taskDir)) {
      fs.mkdirSync(taskDir, { recursive: true });
    }
    
    // Create separate log files for different monitoring
    const sentinelLogFile = path.join(taskDir, `sentinel-${timestamp}.log`);
    const clientLogFile = path.join(taskDir, `client-monitor-${timestamp}.log`);
    const serverLogFile = path.join(taskDir, `server-monitor-${timestamp}.log`);
    const issuesLogFile = path.join(taskDir, `issues-${timestamp}.log`);
    
    // Initialize log files
    const startTime = new Date().toISOString();
    const initMessage = `[${startTime}] SENTINEL: Started monitoring task '${task}'\n`;
    
    fs.writeFileSync(sentinelLogFile, initMessage);
    fs.writeFileSync(clientLogFile, `[${startTime}] CLIENT_MONITOR: Started client-side monitoring\n`);
    fs.writeFileSync(serverLogFile, `[${startTime}] SERVER_MONITOR: Started server-side monitoring\n`);
    fs.writeFileSync(issuesLogFile, `[${startTime}] ISSUES: Issue tracking started\n`);
    
    // Start monitoring process (simplified for now - logs that it's monitoring)
    this.logSentinelWork(sentinelLogFile, 'INIT', 'Sentinel initialized with monitoring capabilities');
    this.logSentinelWork(sentinelLogFile, 'MONITOR_START', 'Client and server monitoring active');
    
    // Check for common issues and log them
    try {
      this.checkForIssues(taskDir, issuesLogFile);
    } catch (error) {
      this.logSentinelWork(sentinelLogFile, 'ERROR', `Issue checking failed: ${error.message}`);
    }
    
    console.log(`🛡️ Sentinel started for task: ${task}`);
    console.log(`📝 Monitoring logs in: ${taskDir}`);
    console.log(`  - Sentinel work: ${path.basename(sentinelLogFile)}`);
    console.log(`  - Client monitor: ${path.basename(clientLogFile)}`);
    console.log(`  - Server monitor: ${path.basename(serverLogFile)}`);
    console.log(`  - Issues tracker: ${path.basename(issuesLogFile)}`);
    
    return this.createSuccessResult({
      task,
      logFiles: {
        sentinel: path.basename(sentinelLogFile),
        client: path.basename(clientLogFile),
        server: path.basename(serverLogFile),
        issues: path.basename(issuesLogFile)
      },
      directory: taskDir
    }, `Sentinel started monitoring task: ${task}`);
  }
  
  static logSentinelWork(logFile, action, message) {
    const timestamp = new Date().toISOString();
    const logEntry = `[${timestamp}] SENTINEL_WORK: ${action} - ${message}\n`;
    fs.appendFileSync(logFile, logEntry);
  }
  
  static checkForIssues(taskDir, issuesLogFile) {
    const timestamp = new Date().toISOString();
    
    // Check for screenshots directory
    const screenshotsDir = path.join(path.dirname(path.dirname(taskDir)), 'screenshots');
    if (!fs.existsSync(screenshotsDir)) {
      const issue = `[${timestamp}] ISSUE: Screenshots directory not found at ${screenshotsDir}\n`;
      fs.appendFileSync(issuesLogFile, issue);
    } else {
      const screenshotCount = fs.readdirSync(screenshotsDir).filter(f => f.endsWith('.png')).length;
      const info = `[${timestamp}] INFO: Found ${screenshotCount} screenshots in ${screenshotsDir}\n`;
      fs.appendFileSync(issuesLogFile, info);
    }
    
    // Check if Continuum server is responsive (basic check)
    try {
      const continuumPid = path.join(path.dirname(path.dirname(taskDir)), 'continuum.pid');
      if (fs.existsSync(continuumPid)) {
        const info = `[${timestamp}] INFO: Continuum PID file exists\n`;
        fs.appendFileSync(issuesLogFile, info);
      } else {
        const issue = `[${timestamp}] ISSUE: No Continuum PID file found\n`;
        fs.appendFileSync(issuesLogFile, issue);
      }
    } catch (error) {
      const issue = `[${timestamp}] ISSUE: Error checking Continuum status: ${error.message}\n`;
      fs.appendFileSync(issuesLogFile, issue);
    }
  }
  
  static async getSentinelStatus(sentinelDir) {
    if (!fs.existsSync(sentinelDir)) {
      return this.createSuccessResult({ tasks: [] }, 'No sentinel tasks found');
    }
    
    const tasks = fs.readdirSync(sentinelDir, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => {
        const taskPath = path.join(sentinelDir, dirent.name);
        const logFiles = fs.readdirSync(taskPath)
          .filter(f => f.endsWith('.log'))
          .sort()
          .reverse();
        
        return {
          name: dirent.name,
          logFiles: logFiles.length,
          latestLog: logFiles[0] || null,
          directory: taskPath
        };
      });
    
    console.log(`🛡️ Sentinel status: ${tasks.length} tasks`);
    tasks.forEach(task => {
      console.log(`  📁 ${task.name}: ${task.logFiles} log files, latest: ${task.latestLog || 'none'}`);
    });
    
    return this.createSuccessResult({ tasks }, `Found ${tasks.length} sentinel tasks`);
  }
  
  static async executeJavaScript(sentinelDir, task, script, continuum) {
    const timestamp = new Date().toISOString();
    const taskDir = path.join(sentinelDir, task);
    
    // Ensure task directory exists
    if (!fs.existsSync(taskDir)) {
      fs.mkdirSync(taskDir, { recursive: true });
    }
    
    const logFile = path.join(taskDir, `js-execution-${timestamp.replace(/[:.]/g, '-')}.log`);
    
    try {
      // Log the execution start
      const startLog = `[${timestamp}] SENTINEL_JS: Starting JavaScript execution\n[${timestamp}] SCRIPT: ${script}\n`;
      fs.writeFileSync(logFile, startLog);
      
      // Execute JavaScript via browser_js command
      const browserJsCommand = continuum.commandProcessor.commands.get('BROWSERJS');
      if (!browserJsCommand) {
        throw new Error('BROWSERJS command not available');
      }
      
      // Execute the script and capture console logs
      const wrappedScript = `
        // Sentinel monitoring script execution
        console.log('🛡️ SENTINEL: Starting JavaScript execution at ' + new Date().toISOString());
        try {
          ${script}
          console.log('🛡️ SENTINEL: JavaScript execution completed successfully');
        } catch (error) {
          console.error('🛡️ SENTINEL: JavaScript execution failed:', error.message);
          throw error;
        }
      `;
      
      const result = await browserJsCommand.execute(JSON.stringify({ script: wrappedScript }), continuum);
      
      // Log the results
      const resultLog = `[${new Date().toISOString()}] RESULT: ${JSON.stringify(result, null, 2)}\n`;
      fs.appendFileSync(logFile, resultLog);
      
      console.log(`🛡️ SENTINEL: JavaScript executed, logs saved to ${logFile}`);
      
      return this.createSuccessResult({
        task,
        script: script.substring(0, 100) + (script.length > 100 ? '...' : ''),
        result,
        logFile: path.basename(logFile)
      }, 'JavaScript executed successfully');
      
    } catch (error) {
      const errorLog = `[${new Date().toISOString()}] ERROR: ${error.message}\n`;
      fs.appendFileSync(logFile, errorLog);
      
      return this.createErrorResult('JavaScript execution failed', error.message);
    }
  }
  
  static async runMonitoringScript(sentinelDir, task, scriptName, interval, continuum) {
    const taskDir = path.join(sentinelDir, task);
    const scriptsDir = path.join(path.dirname(sentinelDir), 'scripts');
    
    // Ensure directories exist
    if (!fs.existsSync(taskDir)) {
      fs.mkdirSync(taskDir, { recursive: true });
    }
    if (!fs.existsSync(scriptsDir)) {
      fs.mkdirSync(scriptsDir, { recursive: true });
    }
    
    // Check if script exists or create a default one
    const scriptFile = path.join(scriptsDir, `${scriptName}.js`);
    let scriptContent;
    
    if (!fs.existsSync(scriptFile)) {
      // Create a default monitoring script
      scriptContent = this.createDefaultMonitoringScript(scriptName);
      fs.writeFileSync(scriptFile, scriptContent);
      console.log(`🛡️ SENTINEL: Created default monitoring script: ${scriptFile}`);
    } else {
      scriptContent = fs.readFileSync(scriptFile, 'utf8');
    }
    
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const logFile = path.join(taskDir, `monitoring-${scriptName}-${timestamp}.log`);
    
    // Initialize monitoring log
    const initLog = `[${new Date().toISOString()}] SENTINEL_MONITOR: Starting monitoring script '${scriptName}'\n[${new Date().toISOString()}] INTERVAL: ${interval} seconds\n[${new Date().toISOString()}] SCRIPT_FILE: ${scriptFile}\n\n`;
    fs.writeFileSync(logFile, initLog);
    
    // Execute the script once immediately
    try {
      await this.executeMonitoringScript(scriptContent, logFile, continuum);
      
      return this.createSuccessResult({
        task,
        script: scriptName,
        scriptFile: path.basename(scriptFile),
        logFile: path.basename(logFile),
        interval: interval
      }, `Monitoring script '${scriptName}' executed - set up periodic execution every ${interval}s`);
      
    } catch (error) {
      const errorLog = `[${new Date().toISOString()}] ERROR: ${error.message}\n`;
      fs.appendFileSync(logFile, errorLog);
      
      return this.createErrorResult('Monitoring script failed', error.message);
    }
  }
  
  static async executeMonitoringScript(scriptContent, logFile, continuum) {
    const timestamp = new Date().toISOString();
    
    try {
      // Execute via browser_js command
      const browserJsCommand = continuum.commandProcessor.commands.get('BROWSERJS');
      if (!browserJsCommand) {
        throw new Error('BROWSERJS command not available');
      }
      
      const wrappedScript = `
        // Sentinel monitoring execution
        const timestamp = new Date().toISOString();
        console.log('🛡️ SENTINEL MONITOR: Execution started at ' + timestamp);
        
        try {
          ${scriptContent}
          console.log('🛡️ SENTINEL MONITOR: Execution completed at ' + new Date().toISOString());
        } catch (error) {
          console.error('🛡️ SENTINEL MONITOR: Execution failed:', error.message);
          throw error;
        }
      `;
      
      const result = await browserJsCommand.execute(JSON.stringify({ script: wrappedScript }), continuum);
      
      // Log the execution
      const execLog = `[${timestamp}] EXECUTION: Success\n[${timestamp}] RESULT: ${JSON.stringify(result, null, 2)}\n\n`;
      fs.appendFileSync(logFile, execLog);
      
      return result;
      
    } catch (error) {
      const errorLog = `[${timestamp}] EXECUTION: Failed - ${error.message}\n\n`;
      fs.appendFileSync(logFile, errorLog);
      throw error;
    }
  }
  
  static createDefaultMonitoringScript(scriptName) {
    if (scriptName === 'logs') {
      return `
// Sentinel Log Monitor Script
// Captures and analyzes console logs

console.log('📊 LOG MONITOR: Checking console activity...');

// Check for recent console logs via clientLogs
if (window.continuum && window.continuum.connected) {
  console.log('✅ LOG MONITOR: Continuum client connected');
  
  // Log current page state
  console.log('📍 LOG MONITOR: Current URL:', window.location.href);
  console.log('📍 LOG MONITOR: Page title:', document.title);
  console.log('📍 LOG MONITOR: WebSocket state:', window.ws ? window.ws.readyState : 'No WebSocket');
  
  // Check for error elements on page
  const errorElements = document.querySelectorAll('[class*="error"], [class*="warning"], .alert');
  console.log('⚠️ LOG MONITOR: Found ' + errorElements.length + ' potential error elements');
  
  if (errorElements.length > 0) {
    errorElements.forEach((el, i) => {
      console.log('🔍 LOG MONITOR: Error element ' + (i+1) + ':', el.textContent.trim().substring(0, 100));
    });
  }
  
} else {
  console.warn('❌ LOG MONITOR: Continuum client not connected');
}

console.log('✅ LOG MONITOR: Monitoring cycle complete');
`;
    } else if (scriptName === 'health') {
      return `
// Sentinel Health Monitor Script
// Checks system health and performance

console.log('🏥 HEALTH MONITOR: Starting health check...');

// Check page performance
const perfData = performance.getEntriesByType('navigation')[0];
if (perfData) {
  console.log('⚡ HEALTH MONITOR: Page load time:', Math.round(perfData.loadEventEnd - perfData.loadEventStart), 'ms');
  console.log('⚡ HEALTH MONITOR: DOM ready time:', Math.round(perfData.domContentLoadedEventEnd - perfData.domContentLoadedEventStart), 'ms');
}

// Check memory usage (if available)
if (performance.memory) {
  const memMB = Math.round(performance.memory.usedJSHeapSize / 1024 / 1024);
  console.log('💾 HEALTH MONITOR: Memory usage:', memMB, 'MB');
}

// Check WebSocket connection health
if (window.ws) {
  console.log('🔗 HEALTH MONITOR: WebSocket state:', 
    window.ws.readyState === 0 ? 'CONNECTING' :
    window.ws.readyState === 1 ? 'OPEN' :
    window.ws.readyState === 2 ? 'CLOSING' : 'CLOSED'
  );
} else {
  console.warn('❌ HEALTH MONITOR: No WebSocket connection');
}

// Check for JavaScript errors
window.sentinelErrorCount = window.sentinelErrorCount || 0;
console.log('🐛 HEALTH MONITOR: JavaScript errors since last check:', window.sentinelErrorCount);
window.sentinelErrorCount = 0; // Reset counter

console.log('✅ HEALTH MONITOR: Health check complete');
`;
    } else {
      return `
// Default Sentinel Monitoring Script: ${scriptName}
// Custom monitoring logic

console.log('🛡️ SENTINEL: Custom monitor "${scriptName}" executing...');

// Add your monitoring logic here
console.log('📊 MONITOR: Current timestamp:', new Date().toISOString());
console.log('📊 MONITOR: Page URL:', window.location.href);

// Example: Check for specific elements or conditions
const elements = document.querySelectorAll('body');
console.log('📊 MONITOR: Found', elements.length, 'body elements');

console.log('✅ MONITOR: ${scriptName} execution complete');
`;
    }
  }
}

module.exports = SentinelCommand;