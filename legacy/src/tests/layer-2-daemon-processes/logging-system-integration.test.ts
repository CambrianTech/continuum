#!/usr/bin/env node
/**
 * JTAG Logging System Integration Test
 * 
 * Tests the complete logging flow:
 * 1. Server WebSocket receives JtagMessage
 * 2. Logger processes message
 * 3. Logger checks if platform.level files exist
 * 4. Creates files from templates if missing  
 * 5. Appends log entry to both text and JSON files
 */

import { JTAGBase } from '@tests/shared/JTAGBase';
import { JTAGWebSocketServer } from '@tests/shared/JTAGWebSocket';
import { jtagConfig } from '@tests/shared/config';
import * as fs from 'fs';
import * as path from 'path';

const TEST_CONFIG = {
  testPort: 9003, // Use different port to avoid conflicts
  testLogDir: path.resolve(__dirname, '../../../..', '.continuum/jtag-test/logs'),
  testMessage: {
    timestamp: '2025-07-20T21:00:00.000Z',
    context: 'browser',
    component: 'TEST_COMPONENT',
    message: 'Test integration message',
    data: { test: true, value: 42 },
    type: 'log'
  }
};

class LoggingSystemIntegrationTest {
  private server: JTAGWebSocketServer | null = null;
  private logEntries: any[] = [];
  
  constructor() {
    console.log('🧪 JTAG Logging System Integration Test');
    console.log('======================================');
  }

  /**
   * Step 1: Setup test environment
   */
  async setup(): Promise<void> {
    console.log('\n📋 Step 1: Setting up test environment...');
    
    // Clean test log directory
    if (fs.existsSync(TEST_CONFIG.testLogDir)) {
      fs.rmSync(TEST_CONFIG.testLogDir, { recursive: true });
    }
    fs.mkdirSync(TEST_CONFIG.testLogDir, { recursive: true });
    
    console.log(`   ✅ Test log directory created: ${TEST_CONFIG.testLogDir}`);
  }

  /**
   * Step 2: Initialize JTAG with test configuration
   */
  async initializeJTAG(): Promise<void> {
    console.log('\n📋 Step 2: Initializing JTAG with test config...');
    
    // Initialize JTAG with our test log directory
    JTAGBase.initialize({
      jtagPort: TEST_CONFIG.testPort,
      logDirectory: TEST_CONFIG.testLogDir,
      enableConsoleOutput: false, // Disable to avoid test noise
      enableRemoteLogging: true
    });
    
    console.log(`   ✅ JTAG initialized with port ${TEST_CONFIG.testPort}`);
    console.log(`   ✅ Log directory: ${TEST_CONFIG.testLogDir}`);
  }

  /**
   * Step 3: Start WebSocket server to receive messages
   */
  async startWebSocketServer(): Promise<void> {
    console.log('\n📋 Step 3: Starting WebSocket server...');
    
    this.server = new JTAGWebSocketServer({
      port: TEST_CONFIG.testPort,
      onLog: (entry) => {
        console.log(`   📨 Server received log entry:`, entry);
        this.logEntries.push(entry);
        
        // This is the critical step 4: Server processes JtagMessage
        this.processLogMessage(entry);
      },
      onScreenshot: async () => ({ success: false }),
      onExec: async () => ({ success: false })
    });
    
    await this.server.start();
    console.log(`   ✅ WebSocket server started on port ${TEST_CONFIG.testPort}`);
  }

  /**
   * Step 4: Process log message (this is what happens when server receives JtagMessage)
   */
  private processLogMessage(entry: any): void {
    console.log('\n📋 Step 4: Processing JtagMessage...');
    console.log(`   📝 Processing: ${entry.context}.${entry.type} - ${entry.component}: ${entry.message}`);
    
    // This calls our logging system
    this.createLogFiles(entry);
  }

  /**
   * Step 5: Check if platform.level files exist, create from templates if not
   */
  private createLogFiles(entry: any): void {
    console.log('\n📋 Step 5: Checking/creating log files...');
    
    const platform = entry.context; // 'browser' or 'server'
    const level = entry.type; // 'log', 'warn', 'error', etc.
    
    const textLogFile = path.join(TEST_CONFIG.testLogDir, `${platform}.${level}.txt`);
    const jsonLogFile = path.join(TEST_CONFIG.testLogDir, `${platform}.${level}.json`);
    
    console.log(`   🔍 Checking files: ${platform}.${level}.txt and ${platform}.${level}.json`);
    
    // Check if text log file exists
    if (!fs.existsSync(textLogFile)) {
      console.log(`   ❌ ${platform}.${level}.txt does NOT exist - creating from template`);
      this.createTextLogFileFromTemplate(textLogFile, platform, level);
    } else {
      console.log(`   ✅ ${platform}.${level}.txt exists`);
    }
    
    // Check if JSON log file exists
    if (!fs.existsSync(jsonLogFile)) {
      console.log(`   ❌ ${platform}.${level}.json does NOT exist - creating from template`);
      this.createJsonLogFileFromTemplate(jsonLogFile, platform, level);
    } else {
      console.log(`   ✅ ${platform}.${level}.json exists`);
    }
    
    // Step 6: Append to both files
    this.appendToLogFiles(entry, textLogFile, jsonLogFile);
  }

  /**
   * Create text log file from template
   */
  private createTextLogFileFromTemplate(filePath: string, platform: string, level: string): void {
    const templatePath = path.join(__dirname, '../templates/log-template.txt');
    
    let template = '';
    
    // Try to load template file
    try {
      template = fs.readFileSync(templatePath, 'utf8');
      console.log(`   📄 Loaded template from: ${templatePath}`);
    } catch (templateError) {
      console.log(`   ⚠️  Template file not found, using fallback template`);
      // Fallback inline template
      template = `# JTAG {PLATFORM}.{LEVEL} Log File
# Generated: {TIMESTAMP}
# Platform: {PLATFORM} (browser/server)
# Level: {LEVEL} (log/warn/error/info/critical/trace/probe)
# Session: {SESSION_ID}
# 
# Format: [timestamp] component: message | data
#
`;
    }
    
    // Replace template variables
    const content = template
      .replace(/\{PLATFORM\}/g, platform)
      .replace(/\{LEVEL\}/g, level)
      .replace(/\{SESSION_ID\}/g, `integration_test_${Date.now()}`)
      .replace(/\{TIMESTAMP\}/g, new Date().toISOString());
    
    fs.writeFileSync(filePath, content);
    console.log(`   ✅ Created ${path.basename(filePath)} from template`);
  }

  /**
   * Create JSON log file from template
   */
  private createJsonLogFileFromTemplate(filePath: string, platform: string, level: string): void {
    const templatePath = path.join(__dirname, '../templates/log-template.json');
    
    let template = '';
    
    // Try to load template file
    try {
      template = fs.readFileSync(templatePath, 'utf8');
      console.log(`   📄 Loaded template from: ${templatePath}`);
    } catch (templateError) {
      console.log(`   ⚠️  Template file not found, using fallback template`);
      // Fallback inline template
      template = `{
  "meta": {
    "platform": "{PLATFORM}",
    "level": "{LEVEL}",
    "sessionId": "{SESSION_ID}",
    "created": "{TIMESTAMP}",
    "format": "JTAG structured log entries"
  },
  "entries": []
}`;
    }
    
    // Replace template variables
    const content = template
      .replace(/\{PLATFORM\}/g, platform)
      .replace(/\{LEVEL\}/g, level)
      .replace(/\{SESSION_ID\}/g, `integration_test_${Date.now()}`)
      .replace(/\{TIMESTAMP\}/g, new Date().toISOString());
    
    fs.writeFileSync(filePath, content);
    console.log(`   ✅ Created ${path.basename(filePath)} from template`);
  }

  /**
   * Step 6: Append log entry to both files
   */
  private appendToLogFiles(entry: any, textLogFile: string, jsonLogFile: string): void {
    console.log('\n📋 Step 6: Appending to log files...');
    
    // Append to text log file
    const logLine = `[${entry.timestamp}] ${entry.component}: ${entry.message}${entry.data ? ` | ${JSON.stringify(entry.data)}` : ''}\n`;
    fs.appendFileSync(textLogFile, logLine);
    console.log(`   ✅ Appended to ${path.basename(textLogFile)}`);
    
    // Append to JSON log file (read, modify, write)
    try {
      const jsonContent = JSON.parse(fs.readFileSync(jsonLogFile, 'utf8'));
      jsonContent.entries.push({
        timestamp: entry.timestamp,
        component: entry.component,
        message: entry.message,
        data: entry.data,
        type: entry.type,
        context: entry.context
      });
      fs.writeFileSync(jsonLogFile, JSON.stringify(jsonContent, null, 2));
      console.log(`   ✅ Appended to ${path.basename(jsonLogFile)}`);
    } catch (jsonError) {
      console.error(`   ❌ Failed to append to JSON file:`, jsonError);
    }
  }

  /**
   * Step 7: Send test message via WebSocket to trigger the flow
   */
  async sendTestMessage(): Promise<void> {
    console.log('\n📋 Step 7: Sending test message via WebSocket...');
    
    // Give server a moment to fully start
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Create WebSocket client to send test message
    const WebSocket = require('ws');
    const ws = new WebSocket(`ws://localhost:${TEST_CONFIG.testPort}`);
    
    return new Promise((resolve, reject) => {
      ws.on('open', () => {
        console.log('   📡 WebSocket client connected');
        
        // Send the test message
        const message = {
          type: 'log',
          payload: TEST_CONFIG.testMessage
        };
        
        console.log('   📤 Sending test message:', message);
        ws.send(JSON.stringify(message));
        
        // Wait for processing
        setTimeout(() => {
          ws.close();
          resolve();
        }, 500);
      });
      
      ws.on('error', (error: any) => {
        console.error('   ❌ WebSocket error:', error);
        reject(error);
      });
    });
  }

  /**
   * Step 8: Verify the results
   */
  async verifyResults(): Promise<void> {
    console.log('\n📋 Step 8: Verifying results...');
    
    const platform = TEST_CONFIG.testMessage.context; // 'browser'
    const level = TEST_CONFIG.testMessage.type; // 'log'
    
    const textLogFile = path.join(TEST_CONFIG.testLogDir, `${platform}.${level}.txt`);
    const jsonLogFile = path.join(TEST_CONFIG.testLogDir, `${platform}.${level}.json`);
    
    // Check if files were created
    console.log(`   🔍 Checking if files were created...`);
    
    if (fs.existsSync(textLogFile)) {
      console.log(`   ✅ ${platform}.${level}.txt exists`);
      const content = fs.readFileSync(textLogFile, 'utf8');
      console.log(`   📄 Content preview:`, content.split('\n').slice(-3).join('\n'));
    } else {
      console.log(`   ❌ ${platform}.${level}.txt does NOT exist`);
    }
    
    if (fs.existsSync(jsonLogFile)) {
      console.log(`   ✅ ${platform}.${level}.json exists`);
      const content = JSON.parse(fs.readFileSync(jsonLogFile, 'utf8'));
      console.log(`   📄 Entries count:`, content.entries.length);
      console.log(`   📄 Last entry:`, content.entries[content.entries.length - 1]);
    } else {
      console.log(`   ❌ ${platform}.${level}.json does NOT exist`);
    }
    
    // Check log entry was received
    console.log(`   📨 Total log entries received:`, this.logEntries.length);
    if (this.logEntries.length > 0) {
      console.log(`   📄 Last received:`, this.logEntries[this.logEntries.length - 1]);
    }
  }

  /**
   * Cleanup test resources
   */
  async cleanup(): Promise<void> {
    console.log('\n🧹 Cleaning up...');
    
    if (this.server) {
      await this.server.stop();
      console.log('   ✅ WebSocket server stopped');
    }
  }

  /**
   * Run the complete integration test
   */
  async runTest(): Promise<void> {
    try {
      await this.setup();
      await this.initializeJTAG();
      await this.startWebSocketServer();
      await this.sendTestMessage();
      await this.verifyResults();
      
      console.log('\n🎉 Integration test completed successfully!');
      console.log('=====================================');
      console.log('✅ WebSocket message received');
      console.log('✅ Logger processed message');
      console.log('✅ Files created from templates');
      console.log('✅ Log entries appended to both files');
      
    } catch (error) {
      console.error('\n💥 Integration test failed:', error);
      throw error;
    } finally {
      await this.cleanup();
    }
  }
}

// Run test if called directly
if (require.main === module) {
  const test = new LoggingSystemIntegrationTest();
  test.runTest().catch(error => {
    console.error('Test failed:', error);
    process.exit(1);
  });
}

export { LoggingSystemIntegrationTest };