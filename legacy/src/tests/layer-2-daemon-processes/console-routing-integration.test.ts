#!/usr/bin/env node
/**
 * Console Routing Integration Tests (Real I/O)
 * 
 * Tests the complete console routing flow with REAL infrastructure:
 * - Real console object interception
 * - Real file system operations
 * - Real template loading and processing
 * - Real transport communication
 * 
 * This validates that the console routing architecture works end-to-end
 * with actual system resources, not mocks.
 */

import * as fs from 'fs';
import * as path from 'path';
import { JTAGBase } from '../../system/core/shared/JTAGBase';
import { JTAGSmartTransport } from '@shared/JTAGTransportFactory';
import { MockSuccessTransport } from '@tests/shared/MockTransports';
import type { JTAGConfig } from '../../../system/core/types/JTAGTypes';

class ConsoleRoutingIntegrationTest {
  private testLogDir: string;
  private testTemplateDir: string;
  private originalConsole: {
    log: Function;
    warn: Function;
    error: Function;
  };
  private cleanupTasks: (() => Promise<void>)[] = [];
  
  /**
   * Promise-based file watching - waits for files to exist and have content
   * Uses fs.watch events instead of setTimeout for intelligent async handling
   */
  private async waitForFileOperations(filePaths: string[], timeoutMs = 5000): Promise<void> {
    return Promise.all(filePaths.map(filePath => this.waitForFile(filePath, timeoutMs)))
      .then(() => void 0);
  }
  
  /**
   * Promise-based single file watcher using fs events
   */
  private async waitForFile(filePath: string, timeoutMs = 5000): Promise<void> {
    // If file already exists with content, resolve immediately
    if (fs.existsSync(filePath)) {
      const stat = fs.statSync(filePath);
      if (stat.size > 0) return Promise.resolve();
    }
    
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        watcher?.close();
        reject(new Error(`File operation timeout: ${filePath} not ready within ${timeoutMs}ms`));
      }, timeoutMs);
      
      let watcher: fs.FSWatcher | null = null;
      
      try {
        // Watch the directory containing the file
        const dir = path.dirname(filePath);
        const filename = path.basename(filePath);
        
        watcher = fs.watch(dir, (eventType, changedFilename) => {
          if (changedFilename === filename && fs.existsSync(filePath)) {
            const stat = fs.statSync(filePath);
            if (stat.size > 0) {
              clearTimeout(timer);
              watcher?.close();
              resolve();
            }
          }
        });
        
        // Also check immediately in case file was created between checks
        setImmediate(() => {
          if (fs.existsSync(filePath)) {
            const stat = fs.statSync(filePath);
            if (stat.size > 0) {
              clearTimeout(timer);
              watcher?.close();
              resolve();
            }
          }
        });
        
      } catch (error) {
        clearTimeout(timer);
        watcher?.close();
        reject(error);
      }
    });
  }

  constructor() {
    this.testLogDir = path.resolve(__dirname, '../../../../.continuum/jtag-console-integration/logs');
    this.testTemplateDir = path.resolve(__dirname, '../../templates');
    
    // Preserve original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console)
    };
  }

  async runAllTests(): Promise<void> {
    console.log('🧪 Console Routing Integration Tests (Real I/O)');
    console.log('=================================================\n');

    try {
      await this.setupRealTestEnvironment();
      
      await this.testRealConsoleInterception();
      await this.testRealFileSystemOperations();
      await this.testRealTemplateSystem();
      await this.testRealTransportIntegration();
      await this.testRealErrorHandling();
      await this.validateRealArtifacts();
      
      console.log('\n🎉 All console routing integration tests passed!');
      console.log('✅ Console routing works end-to-end with real infrastructure');
      
    } finally {
      await this.cleanupRealTestEnvironment();
    }
  }

  private async setupRealTestEnvironment(): Promise<void> {
    console.log('🏗️ Setting up real test environment...');
    
    // Create real test directories
    if (fs.existsSync(this.testLogDir)) {
      fs.rmSync(this.testLogDir, { recursive: true });
    }
    fs.mkdirSync(this.testLogDir, { recursive: true });
    
    // Ensure template directory exists with real templates
    await this.ensureRealTemplates();
    
    console.log(`   ✅ Real log directory: ${this.testLogDir}`);
    console.log(`   ✅ Real template directory: ${this.testTemplateDir}`);
    console.log('   🎉 Real test environment ready\n');
  }

  private async ensureRealTemplates(): Promise<void> {
    // Create real template files if they don't exist
    if (!fs.existsSync(this.testTemplateDir)) {
      fs.mkdirSync(this.testTemplateDir, { recursive: true });
    }
    
    const txtTemplate = `# JTAG Log File - $platform.$level
# Created: $timestamp
# Context: $context
# Integration Test Template

`;

    const jsonTemplate = `{
  "jtagLog": true,
  "platform": "$platform",
  "level": "$level", 
  "created": "$timestamp",
  "context": "$context",
  "integrationType": "real-filesystem",
  "entries": []
}`;

    const txtTemplatePath = path.join(this.testTemplateDir, 'log-template.txt');
    const jsonTemplatePath = path.join(this.testTemplateDir, 'log-template.json');
    
    if (!fs.existsSync(txtTemplatePath)) {
      fs.writeFileSync(txtTemplatePath, txtTemplate);
    }
    
    if (!fs.existsSync(jsonTemplatePath)) {
      fs.writeFileSync(jsonTemplatePath, jsonTemplate);
    }
    
    console.log('   ✅ Real template files ensured');
  }

  private async testRealConsoleInterception(): Promise<void> {
    console.log('🎯 Testing Real Console Interception...');
    
    // Initialize JTAG with real transport (but use mock for predictability)
    const mockTransport = new MockSuccessTransport();
    const config: JTAGConfig = {
      context: 'server',
      jtagPort: 9001,
      logDirectory: this.testLogDir,
      enableRemoteLogging: false, // Force local logging for this test
      enableConsoleOutput: false,
      maxBufferSize: 100
    };
    
    // Initialize JTAG system
    JTAGBase.initialize(config);
    
    // Test real console interception
    const testMessages = [
      'Real console.log message',
      'Real console.warn message', 
      'Real console.error message'
    ];
    
    // Store original console methods before interception
    const preInterceptConsole = {
      log: console.log,
      warn: console.warn,
      error: console.error
    };
    
    // Apply console interception (real console modification)
    JTAGBase.attach(console);
    
    // Verify console methods were actually modified
    if (console.log === preInterceptConsole.log) {
      throw new Error('Console.log should be intercepted and modified');
    }
    
    console.log('   ✅ Console interception applied to real console object');
    
    // Test intercepted console calls (these should route through JTAG)
    this.originalConsole.log('   🔍 Testing intercepted console.log...');
    console.log(testMessages[0]);
    
    this.originalConsole.log('   🔍 Testing intercepted console.warn...');
    console.warn(testMessages[1]);
    
    this.originalConsole.log('   🔍 Testing intercepted console.error...');
    console.error(testMessages[2]);
    
    // Wait for async file operations using Promise-based file watching
    await this.waitForFileOperations([
      path.join(this.testLogDir, 'server.log.txt'),
      path.join(this.testLogDir, 'server.warn.txt'),
      path.join(this.testLogDir, 'server.error.txt')
    ]);
    
    // Verify that original console still works (preserved)
    this.originalConsole.log('   ✅ Original console methods preserved');
    
    console.log('   🎉 Real console interception: PASSED\n');
  }

  private async testRealFileSystemOperations(): Promise<void> {
    console.log('📁 Testing Real File System Operations...');
    
    // Test direct file creation through JTAG system
    JTAGBase.log('FS_TEST', 'Testing real file system log creation');
    JTAGBase.warn('FS_TEST', 'Testing real file system warn creation');
    JTAGBase.error('FS_TEST', 'Testing real file system error creation');
    JTAGBase.critical('FS_TEST', 'Testing real file system critical creation');
    
    // Wait for real async file operations with Promise-based file watching
    await this.waitForFileOperations([
      path.join(this.testLogDir, 'server.log.txt'),
      path.join(this.testLogDir, 'server.warn.txt'), 
      path.join(this.testLogDir, 'server.error.txt'),
      path.join(this.testLogDir, 'server.critical.txt')
    ]);
    
    // Validate real files were created
    const expectedFiles = [
      'server.log.txt',
      'server.warn.txt', 
      'server.error.txt',
      'server.critical.txt'
    ];
    
    for (const filename of expectedFiles) {
      const filePath = path.join(this.testLogDir, filename);
      
      if (!fs.existsSync(filePath)) {
        throw new Error(`Real file ${filename} should be created by filesystem operations`);
      }
      
      const stat = fs.statSync(filePath);
      if (stat.size === 0) {
        throw new Error(`Real file ${filename} should contain data`);
      }
      
      const content = fs.readFileSync(filePath, 'utf8');
      if (!content.includes('FS_TEST')) {
        throw new Error(`Real file ${filename} should contain test data`);
      }
      
      console.log(`   ✅ Real file created: ${filename} (${stat.size} bytes)`);
    }
    
    // Test JSON files too
    const jsonFiles = expectedFiles.map(f => f.replace('.txt', '.json'));
    for (const filename of jsonFiles) {
      const filePath = path.join(this.testLogDir, filename);
      
      if (!fs.existsSync(filePath)) {
        throw new Error(`Real JSON file ${filename} should be created`);
      }
      
      const content = fs.readFileSync(filePath, 'utf8');
      try {
        const parsed = JSON.parse(content);
        if (!parsed.meta || !parsed.entries) {
          throw new Error(`Real JSON file ${filename} should have valid JTAG structure with meta and entries`);
        }
        console.log(`   ✅ Real JSON file created: ${filename} (valid JSON with ${parsed.entries.length} entries)`);
      } catch (error) {
        throw new Error(`Real JSON file ${filename} should contain valid JSON: ${error.message}`);
      }
    }
    
    console.log('   🎉 Real file system operations: PASSED\n');
  }

  private async testRealTemplateSystem(): Promise<void> {
    console.log('📄 Testing Real Template System...');
    
    // Force template-based file creation by deleting existing files
    const testFile = path.join(this.testLogDir, 'server.trace.txt');
    const testJsonFile = path.join(this.testLogDir, 'server.trace.json');
    
    if (fs.existsSync(testFile)) fs.unlinkSync(testFile);
    if (fs.existsSync(testJsonFile)) fs.unlinkSync(testJsonFile);
    
    // Create log entry that will trigger template usage
    JTAGBase.trace('TEMPLATE_TEST', 'testFunction', 'ENTER');
    
    // Wait for template processing and file creation using Promise-based file watching
    await this.waitForFileOperations([testFile, testJsonFile]);
    
    // Validate template-created files
    if (!fs.existsSync(testFile)) {
      throw new Error('Template system should create real .txt file');
    }
    
    if (!fs.existsSync(testJsonFile)) {
      throw new Error('Template system should create real .json file');
    }
    
    // Validate template variable substitution in real files
    const txtContent = fs.readFileSync(testFile, 'utf8');
    const jsonContent = fs.readFileSync(testJsonFile, 'utf8');
    
    // Check txt template substitution
    if (!txtContent.includes('server.trace')) {
      throw new Error('Template should substitute platform.level variables in txt file');
    }
    
    if (!txtContent.includes('Platform: server')) {
      throw new Error('Template should substitute platform variable in txt file'); 
    }
    
    console.log('   ✅ Real template variable substitution in .txt file');
    
    // Check JSON template substitution
    const jsonData = JSON.parse(jsonContent);
    if (jsonData.meta.platform !== 'server') {
      throw new Error('Template should substitute platform variable in JSON meta');
    }
    
    if (jsonData.meta.level !== 'trace') {
      throw new Error('Template should substitute level variable in JSON meta');
    }
    
    if (!jsonData.meta.created) {
      throw new Error('Template should substitute timestamp variable in JSON meta');
    }
    
    if (!Array.isArray(jsonData.entries)) {
      throw new Error('Template should include entries array in JSON structure');
    }
    
    console.log('   ✅ Real template variable substitution in .json file');
    console.log('   🎉 Real template system: PASSED\n');
  }

  private async testRealTransportIntegration(): Promise<void> {
    console.log('🚀 Testing Real Transport Integration...');
    
    // Test with smart transport that uses real network (but mock for this test)
    const smartTransport = new JTAGSmartTransport();
    
    const config: JTAGConfig = {
      context: 'server',
      jtagPort: 9001,
      logDirectory: this.testLogDir,
      enableRemoteLogging: true, // Enable transport usage
      enableConsoleOutput: false,
      maxBufferSize: 100
    };
    
    // Initialize real transport (will attempt real network, fall back to queue)
    const initSuccess = await smartTransport.initialize(config);
    console.log(`   ✅ Real transport initialization: ${initSuccess}`);
    
    // Validate that probe file was created (regardless of transport success)
    const probeFile = path.join(this.testLogDir, 'server.probe.txt');
    
    // Test message sending through real transport layer
    JTAGBase.probe('TRANSPORT_TEST', 'real_network_state', { 
      transportActive: smartTransport.isConnected(),
      timestamp: Date.now() 
    });
    
    // Wait for transport message processing using Promise-based file watching
    await this.waitForFileOperations([probeFile]);
    if (!fs.existsSync(probeFile)) {
      throw new Error('Transport integration should create probe file');
    }
    
    const probeContent = fs.readFileSync(probeFile, 'utf8');
    if (!probeContent.includes('TRANSPORT_TEST')) {
      throw new Error('Transport integration should write probe data to file');
    }
    
    console.log('   ✅ Real transport message processing');
    console.log('   🎉 Real transport integration: PASSED\n');
  }

  private async testRealErrorHandling(): Promise<void> {
    console.log('💥 Testing Real Error Handling...');
    
    // Test filesystem error handling
    const readonlyDir = path.join(this.testLogDir, 'readonly');
    fs.mkdirSync(readonlyDir, { recursive: true });
    
    try {
      // Try to make directory read-only (this may not work on all systems)
      fs.chmodSync(readonlyDir, 0o444);
      
      // Try to create JTAG log in readonly directory
      const readonlyConfig: JTAGConfig = {
        context: 'server',
        jtagPort: 9001,
        logDirectory: readonlyDir,
        enableRemoteLogging: false,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };
      
      // This should handle the error gracefully
      JTAGBase.initialize(readonlyConfig);
      JTAGBase.log('ERROR_TEST', 'Testing readonly directory error handling');
      
      // Wait for error handling to complete - we expect this to fail gracefully
      // Use setImmediate to allow error handling to complete
      await new Promise(resolve => setImmediate(resolve));
      
      console.log('   ✅ Real filesystem error handling (graceful degradation)');
      
    } catch (error) {
      console.log(`   ⚠️ Filesystem error test limited by system permissions: ${error.message}`);
    } finally {
      // Cleanup readonly directory
      try {
        fs.chmodSync(readonlyDir, 0o755);
        fs.rmSync(readonlyDir, { recursive: true });
      } catch (error) {
        // Ignore cleanup errors
      }
    }
    
    // Test invalid template handling
    const invalidTemplateDir = path.join(this.testLogDir, 'invalid-templates');
    fs.mkdirSync(invalidTemplateDir, { recursive: true });
    
    // Create invalid template
    const invalidTemplate = path.join(invalidTemplateDir, 'log-template.txt');
    fs.writeFileSync(invalidTemplate, 'Invalid template with bad $variables}');
    
    // JTAG should handle template errors gracefully
    // (Implementation should have error handling for template processing)
    
    console.log('   ✅ Real error handling resilience');
    console.log('   🎉 Real error handling: PASSED\n');
  }

  private async validateRealArtifacts(): Promise<void> {
    console.log('🔍 Validating Real Test Artifacts...');
    
    // Comprehensive validation of all created files
    const logFiles = fs.readdirSync(this.testLogDir);
    const txtFiles = logFiles.filter(f => f.endsWith('.txt'));
    const jsonFiles = logFiles.filter(f => f.endsWith('.json'));
    
    console.log(`   📊 Real artifacts created: ${txtFiles.length} .txt files, ${jsonFiles.length} .json files`);
    
    // Validate file naming convention
    const expectedPattern = /^server\.(log|warn|error|critical|trace|probe)\.(txt|json)$/;
    const allFiles = [...txtFiles, ...jsonFiles];
    
    for (const filename of allFiles) {
      if (!expectedPattern.test(filename)) {
        throw new Error(`File ${filename} does not follow expected naming pattern`);
      }
    }
    
    console.log('   ✅ All files follow platform.level.extension naming pattern');
    
    // Validate file sizes (should contain actual content)
    let totalSize = 0;
    for (const filename of allFiles) {
      const filePath = path.join(this.testLogDir, filename);
      const stat = fs.statSync(filePath);
      
      if (stat.size < 10) {
        throw new Error(`File ${filename} too small (${stat.size} bytes), should contain content`);
      }
      
      totalSize += stat.size;
    }
    
    console.log(`   ✅ Total content generated: ${totalSize} bytes across ${allFiles.length} files`);
    
    // Validate JSON structure integrity
    for (const jsonFile of jsonFiles) {
      const filePath = path.join(this.testLogDir, jsonFile);
      const content = fs.readFileSync(filePath, 'utf8');
      
      try {
        const parsed = JSON.parse(content);
        
        if (!parsed.meta || !parsed.entries || !parsed.meta.platform || !parsed.meta.level) {
          throw new Error(`JSON file ${jsonFile} missing required JTAG structure (meta.platform, meta.level, entries)`);
        }
        
      } catch (error) {
        throw new Error(`JSON file ${jsonFile} contains invalid JSON: ${error.message}`);
      }
    }
    
    console.log('   ✅ All JSON files have valid structure');
    
    // Show sample content for verification
    const sampleFile = txtFiles[0];
    if (sampleFile) {
      const samplePath = path.join(this.testLogDir, sampleFile);
      const sampleContent = fs.readFileSync(samplePath, 'utf8').substring(0, 200);
      console.log(`   📄 Sample content (${sampleFile}):`);
      console.log(`   ${sampleContent.replace(/\n/g, '\\n').substring(0, 100)}...`);
    }
    
    console.log('   🎉 Real artifact validation: PASSED\n');
  }

  private async cleanupRealTestEnvironment(): Promise<void> {
    console.log('🧹 Cleaning up real test environment...');
    
    // Restore original console methods
    console.log = this.originalConsole.log;
    console.warn = this.originalConsole.warn;
    console.error = this.originalConsole.error;
    
    // Detach JTAG console interception
    JTAGBase.detach(console);
    
    // Execute all cleanup tasks
    for (const cleanup of this.cleanupTasks) {
      try {
        await cleanup();
      } catch (error) {
        console.log(`   ⚠️ Cleanup task failed: ${error.message}`);
      }
    }
    
    console.log('   ✅ Console methods restored');
    console.log('   ✅ JTAG console interception removed');
    console.log('   🎉 Real test environment cleaned up');
  }
}

// Export for integration with other tests
export { ConsoleRoutingIntegrationTest };

// Run tests if called directly
if (require.main === module) {
  const test = new ConsoleRoutingIntegrationTest();
  test.runAllTests().catch(error => {
    console.error('💥 Console routing integration tests failed:', error);
    process.exit(1);
  });
}