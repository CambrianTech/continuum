/**
 * Emergency JTAG - Bulletproof logging for system debugging
 * 
 * When all other logging is broken, this is your tricorder.
 * Writes directly to .continuum/logs with no dependencies.
 */

import * as fs from 'fs';
import * as path from 'path';

export class EmergencyJTAG {
  private static logDir = '.continuum/logs';
  private static initialized = false;

  private static ensureLogDir(): void {
    if (!this.initialized) {
      try {
        if (!fs.existsSync(this.logDir)) {
          fs.mkdirSync(this.logDir, { recursive: true });
        }
        this.initialized = true;
      } catch (error) {
        console.error('EmergencyJTAG: Failed to create log directory:', error);
      }
    }
  }

  /**
   * Log with timestamp and caller info
   */
  static log(component: string, message: string, data?: any): void {
    this.ensureLogDir();
    
    const timestamp = new Date().toISOString();
    const logLine = `[${timestamp}] ${component}: ${message}${data ? ` | ${JSON.stringify(data)}` : ''}\n`;
    
    try {
      // Write to both emergency log and component-specific log
      const emergencyLog = path.join(this.logDir, 'emergency-jtag.log');
      const componentLog = path.join(this.logDir, `${component.toLowerCase()}.emergency.log`);
      
      fs.appendFileSync(emergencyLog, logLine);
      fs.appendFileSync(componentLog, logLine);
      
      // Also console.log for immediate visibility
      console.log(`🚨 EMERGENCY-JTAG [${component}]: ${message}`, data || '');
      
    } catch (error) {
      console.error('EmergencyJTAG: Failed to write log:', error);
    }
  }

  /**
   * Log system state probe
   */
  static probe(component: string, probeName: string, state: any): void {
    this.log(component, `PROBE: ${probeName}`, state);
  }

  /**
   * Log entry/exit of functions
   */
  static trace(component: string, functionName: string, phase: 'ENTER' | 'EXIT', data?: any): void {
    this.log(component, `TRACE: ${functionName} ${phase}`, data);
  }

  /**
   * Log critical system events
   */
  static critical(component: string, event: string, data?: any): void {
    const criticalLog = path.join(this.logDir, 'critical.emergency.log');
    const logLine = `[${new Date().toISOString()}] CRITICAL ${component}: ${event}${data ? ` | ${JSON.stringify(data)}` : ''}\n`;
    
    try {
      fs.appendFileSync(criticalLog, logLine);
      console.error(`🔥 CRITICAL [${component}]: ${event}`, data || '');
    } catch (error) {
      console.error('EmergencyJTAG: Failed to write critical log:', error);
    }
  }

  /**
   * Clear all emergency logs (for clean debugging session)
   */
  static clearLogs(): void {
    this.ensureLogDir();
    try {
      const files = fs.readdirSync(this.logDir);
      for (const file of files) {
        if (file.includes('emergency')) {
          fs.unlinkSync(path.join(this.logDir, file));
        }
      }
      console.log('🚨 EmergencyJTAG: Logs cleared');
    } catch (error) {
      console.error('EmergencyJTAG: Failed to clear logs:', error);
    }
  }
}