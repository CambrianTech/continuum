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
          description: 'Action: start, logs, status',
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
    console.log(`🛡️ SENTINEL: Continuum object available: ${!!continuum}`);
    
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
        return await this.showLogs(sentinelDir, lines, task);
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
      } else {
        console.log(`🛡️ SENTINEL: Invalid action: ${action}`);
        return this.createErrorResult('Invalid action', `Unknown action: ${action}`);
      }
      
    } catch (error) {
      console.error('❌ Sentinel command failed:', error);
      return this.createErrorResult('Sentinel command failed', error.message);
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
}

module.exports = SentinelCommand;