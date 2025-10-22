/**
 * Universal Emergency JTAG - Cross-context debugging system
 * 
 * Works identically in browser and server contexts.
 * Uses port 9001 for client-server JTAG communication.
 * 
 * Philosophy: "One debugging API, two execution contexts"
 */

interface JTAGLogEntry {
  timestamp: string;
  context: 'browser' | 'server';
  component: string;
  message: string;
  data?: any;
  type: 'log' | 'critical' | 'trace' | 'probe';
}

interface JTAGConfig {
  context: 'browser' | 'server';
  jtagPort: number;
  logDirectory?: string;
  enableRemoteLogging: boolean;
}

export class UniversalEmergencyJTAG {
  private static config: JTAGConfig;
  private static initialized = false;
  private static logBuffer: JTAGLogEntry[] = [];
  private static remoteEndpoint: string;

  /**
   * Initialize Emergency JTAG for browser or server context
   */
  static initialize(context: 'browser' | 'server', options?: Partial<JTAGConfig>): void {
    this.config = {
      context,
      jtagPort: 9001,
      logDirectory: context === 'server' ? '.continuum/logs' : undefined,
      enableRemoteLogging: true,
      ...options
    };

    this.remoteEndpoint = `http://localhost:${this.config.jtagPort}/jtag`;
    this.initialized = true;

    if (context === 'server') {
      this.startJTAGServer();
    }

    this.log('JTAG', `Universal Emergency JTAG initialized for ${context} context`);
  }

  /**
   * Universal logging - works same on browser and server
   */
  static log(component: string, message: string, data?: any): void {
    if (!this.initialized) {
      console.warn('Emergency JTAG not initialized - call initialize() first');
      return;
    }

    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: this.config.context,
      component,
      message,
      data,
      type: 'log'
    };

    this.processLogEntry(entry);
  }

  /**
   * Critical event logging
   */
  static critical(component: string, event: string, data?: any): void {
    if (!this.initialized) return;

    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: this.config.context,
      component,
      message: event,
      data,
      type: 'critical'
    };

    this.processLogEntry(entry);
  }

  /**
   * Function tracing
   */
  static trace(component: string, functionName: string, phase: 'ENTER' | 'EXIT', data?: any): void {
    if (!this.initialized) return;

    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: this.config.context,
      component,
      message: `TRACE: ${functionName} ${phase}`,
      data,
      type: 'trace'
    };

    this.processLogEntry(entry);
  }

  /**
   * System state probes
   */
  static probe(component: string, probeName: string, state: any): void {
    if (!this.initialized) return;

    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: this.config.context,
      component,
      message: `PROBE: ${probeName}`,
      data: state,
      type: 'probe'
    };

    this.processLogEntry(entry);
  }

  /**
   * Process log entry - handles both browser and server contexts
   */
  private static processLogEntry(entry: JTAGLogEntry): void {
    // Always console.log for immediate visibility
    const consolePrefix = entry.context === 'browser' ? '🌐 BROWSER-JTAG' : '🚨 SERVER-JTAG';
    console.log(`${consolePrefix} [${entry.component}]: ${entry.message}`, entry.data || '');

    if (this.config.context === 'server') {
      this.writeServerLog(entry);
    } else {
      this.sendToRemoteJTAG(entry);
    }

    // Buffer for potential batch operations
    this.logBuffer.push(entry);
    if (this.logBuffer.length > 1000) {
      this.logBuffer = this.logBuffer.slice(-500); // Keep last 500 entries
    }
  }

  /**
   * Server-side file logging
   */
  private static writeServerLog(entry: JTAGLogEntry): void {
    if (typeof require === 'undefined') return; // Not in Node.js

    try {
      const fs = require('fs');
      const path = require('path');

      if (!this.config.logDirectory) return;

      // Ensure log directory exists
      if (!fs.existsSync(this.config.logDirectory)) {
        fs.mkdirSync(this.config.logDirectory, { recursive: true });
      }

      const logLine = `[${entry.timestamp}] ${entry.context.toUpperCase()} ${entry.component}: ${entry.message}${entry.data ? ` | ${JSON.stringify(entry.data)}` : ''}\n`;
      
      // Write to multiple log files
      const universalLog = path.join(this.config.logDirectory, 'universal-emergency-jtag.log');
      const criticalLog = path.join(this.config.logDirectory, 'critical.emergency.log');
      const componentLog = path.join(this.config.logDirectory, `${entry.component.toLowerCase()}.emergency.log`);

      fs.appendFileSync(universalLog, logLine);
      fs.appendFileSync(componentLog, logLine);

      if (entry.type === 'critical') {
        fs.appendFileSync(criticalLog, logLine);
      }

    } catch (error) {
      console.error('Emergency JTAG: Failed to write server log:', error);
    }
  }

  /**
   * Browser-side remote logging to server
   */
  private static sendToRemoteJTAG(entry: JTAGLogEntry): void {
    if (!this.config.enableRemoteLogging) return;

    try {
      // Use fetch if available (modern browsers)
      if (typeof fetch !== 'undefined') {
        fetch(this.remoteEndpoint, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(entry)
        }).catch(error => {
          // Silent fail - don't spam console if JTAG server is down
        });
      }
      // Could add XMLHttpRequest fallback for older browsers
    } catch (error) {
      // Silent fail - JTAG shouldn't break the application
    }
  }

  /**
   * Start JTAG server on port 9001 (server-side only)
   */
  private static startJTAGServer(): void {
    if (typeof require === 'undefined') return; // Not in Node.js

    try {
      const http = require('http');
      
      const server = http.createServer((req: any, res: any) => {
        // Enable CORS
        res.setHeader('Access-Control-Allow-Origin', '*');
        res.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
        res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

        if (req.method === 'OPTIONS') {
          res.writeHead(200);
          res.end();
          return;
        }

        if (req.method === 'POST' && req.url === '/jtag') {
          let body = '';
          req.on('data', (chunk: any) => body += chunk);
          req.on('end', () => {
            try {
              const entry: JTAGLogEntry = JSON.parse(body);
              // Process browser log entry on server
              this.writeServerLog(entry);
              res.writeHead(200, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ success: true }));
            } catch (error) {
              res.writeHead(400, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ error: 'Invalid JTAG entry' }));
            }
          });
        } else {
          res.writeHead(404);
          res.end();
        }
      });

      server.listen(this.config.jtagPort, () => {
        console.log(`🚨 Emergency JTAG server listening on port ${this.config.jtagPort}`);
      });

    } catch (error) {
      console.error('Failed to start Emergency JTAG server:', error);
    }
  }

  /**
   * Clear all emergency logs
   */
  static clearLogs(): void {
    if (this.config?.context === 'server' && this.config.logDirectory) {
      try {
        const fs = require('fs');
        const files = fs.readdirSync(this.config.logDirectory);
        for (const file of files) {
          if (file.includes('emergency')) {
            const path = require('path');
            fs.unlinkSync(path.join(this.config.logDirectory, file));
          }
        }
        console.log('🚨 Emergency JTAG: Logs cleared');
      } catch (error) {
        console.error('Emergency JTAG: Failed to clear logs:', error);
      }
    }
    this.logBuffer = [];
  }

  /**
   * Get current log buffer (for debugging)
   */
  static getLogs(): JTAGLogEntry[] {
    return [...this.logBuffer];
  }
}