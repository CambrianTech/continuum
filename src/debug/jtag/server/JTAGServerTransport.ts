/**
 * JTAG Server Transport - Server-side transport that writes to files
 */

import { JTAGUniversalMessage } from '../shared/JTAGTypes';
import { JTAGTransportBackend } from '../shared/JTAGRouter';

export class JTAGServerTransport implements JTAGTransportBackend {
  name = 'jtag-server';
  private logDirectory: string;

  constructor(logDirectory?: string) {
    const path = require('path');
    this.logDirectory = logDirectory || path.join(process.cwd(), '.continuum', 'jtag', 'logs');
    this.ensureLogDirectory();
  }

  canHandle(message: JTAGUniversalMessage): boolean {
    // Server transport handles all log messages (from browser and server)
    return message.type === 'log';
  }

  async process(message: JTAGUniversalMessage): Promise<any> {
    const payload = message.payload as any;
    const platform = message.source; // 'browser' or 'server'
    const level = payload.level || 'log';
    const component = payload.component || 'unknown';
    const messageText = payload.message || JSON.stringify(payload);
    const data = payload.data;

    try {
      await this.writeToFile(platform, level, component, messageText, data);
      return { 
        logged: true, 
        platform, 
        level, 
        files: [`${platform}.${level}.txt`, `${platform}.${level}.json`],
        timestamp: new Date().toISOString() 
      };
    } catch (error) {
      console.error('❌ JTAG Server: File logging failed:', error);
      return { 
        logged: false, 
        error: error instanceof Error ? error.message : String(error) 
      };
    }
  }

  private async writeToFile(platform: string, level: string, component: string, message: string, data?: any): Promise<void> {
    const fs = require('fs');
    const path = require('path');
    
    const timestamp = new Date().toISOString();
    
    // Write to platform.level.txt file
    const textFile = path.join(this.logDirectory, `${platform}.${level}.txt`);
    const logLine = `[${timestamp}] ${component}: ${message}${data ? ` | ${JSON.stringify(data)}` : ''}\n`;
    fs.appendFileSync(textFile, logLine);
    
    // Write to platform.level.json file
    const jsonFile = path.join(this.logDirectory, `${platform}.${level}.json`);
    const entry = {
      timestamp,
      component,
      message,
      data,
      level,
      platform
    };
    
    let jsonData: {
      meta: {
        platform: string;
        level: string;
        created?: string;
        format: string;
      };
      entries: any[];
    } = { 
      meta: {
        platform,
        level,
        ...(fs.existsSync(jsonFile) ? {} : { created: timestamp }),
        format: 'JTAG structured log entries'
      },
      entries: [] 
    };
    
    try {
      if (fs.existsSync(jsonFile)) {
        const content = fs.readFileSync(jsonFile, 'utf8');
        jsonData = JSON.parse(content);
      }
    } catch (error) {
      // If JSON is corrupted, start fresh but keep meta
      console.warn(`📝 JTAG Server: JSON file corrupted, recreating: ${jsonFile}`);
    }
    
    jsonData.entries.push(entry);
    fs.writeFileSync(jsonFile, JSON.stringify(jsonData, null, 2));
  }

  private ensureLogDirectory(): void {
    const fs = require('fs');
    
    if (!fs.existsSync(this.logDirectory)) {
      fs.mkdirSync(this.logDirectory, { recursive: true });
      console.log(`📁 JTAG Server: Created log directory: ${this.logDirectory}`);
    }
  }

  isHealthy(): boolean {
    const fs = require('fs');
    try {
      // Test if we can write to the log directory
      fs.accessSync(this.logDirectory, fs.constants.W_OK);
      return true;
    } catch (error) {
      return false;
    }
  }

  getLogDirectory(): string {
    return this.logDirectory;
  }
}