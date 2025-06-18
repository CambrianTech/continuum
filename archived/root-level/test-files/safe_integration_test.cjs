#!/usr/bin/env node
/**
 * Safe Integration Test - Test all commands with log monitoring
 * Checks: command loading, execution, log patterns, error detection
 */

const fs = require('fs');
const path = require('path');

class SafeIntegrationTest {
  constructor() {
    this.results = [];
    this.logBasePath = path.join(__dirname, '.continuum', 'logs');
    this.startTime = Date.now();
  }

  log(message) {
    const timestamp = new Date().toISOString();
    console.log(`[${timestamp}] ${message}`);
  }

  addResult(test, success, details) {
    this.results.push({ test, success, details, timestamp: Date.now() });
    const status = success ? '✅' : '❌';
    this.log(`${status} ${test}: ${details}`);
  }

  // Monitor log files for new entries
  monitorLogs() {
    const errorLogPath = path.join(this.logBasePath, 'browser', `browser-errors-2025-06-16.log`);
    
    if (!fs.existsSync(errorLogPath)) {
      this.addResult('Log Monitoring Setup', false, 'Error log file not found');
      return null;
    }

    // Get current file size
    const initialSize = fs.statSync(errorLogPath).size;
    this.log(`📊 Monitoring logs from position: ${initialSize}`);
    
    return {
      errorLogPath,
      initialSize,
      checkNewErrors: () => {
        const currentSize = fs.statSync(errorLogPath).size;
        if (currentSize > initialSize) {
          const newContent = fs.readFileSync(errorLogPath, 'utf8').slice(initialSize);
          const newErrors = newContent.trim().split('\n').filter(line => line);
          return newErrors.map(line => {
            try {
              return JSON.parse(line);
            } catch {
              return { raw: line };
            }
          });
        }
        return [];
      }
    };
  }

  async testCommandLoading() {
    try {
      this.log('🔧 Testing command loading...');
      
      // Test CoreModule loading
      const CoreModule = require('./src/modules/CoreModule.cjs');
      const coreModule = new CoreModule();
      
      await coreModule.initialize();
      
      const commandCount = coreModule.commands.size;
      const macroCount = coreModule.macros.size;
      
      this.addResult('CoreModule Loading', true, `${commandCount} commands, ${macroCount} macros`);
      
      // Test individual command loading
      const testCommands = ['help', 'agents', 'diagnostics', 'findUser', 'preferences'];
      
      for (const cmdName of testCommands) {
        const cmd = coreModule.getCommand(cmdName);
        if (cmd) {
          this.addResult(`Command Loading: ${cmdName}`, true, 'Loaded successfully');
        } else {
          this.addResult(`Command Loading: ${cmdName}`, false, 'Failed to load');
        }
      }
      
      return coreModule;
      
    } catch (error) {
      this.addResult('Command Loading', false, `Error: ${error.message}`);
      return null;
    }
  }

  async testSafeCommands(coreModule) {
    if (!coreModule) return;
    
    this.log('🔍 Testing safe commands (no side effects)...');
    
    // Test help command
    try {
      const helpCmd = coreModule.getCommand('help');
      const result = await helpCmd.execute('{}', null);
      this.addResult('Help Command Execution', result.success, result.message);
    } catch (error) {
      this.addResult('Help Command Execution', false, error.message);
    }

    // Test findUser command
    try {
      const findUserCmd = coreModule.getCommand('findUser');
      const result = await findUserCmd.execute('{"name": "joel"}', null);
      this.addResult('FindUser Command Execution', result.success, result.message);
    } catch (error) {
      this.addResult('FindUser Command Execution', false, error.message);
    }

    // Test preferences command
    try {
      const prefsCmd = coreModule.getCommand('preferences');
      const testUser = { preferences: { theme: 'dark', mediaInput: 'slack' } };
      const result = await prefsCmd.execute(JSON.stringify({ input: testUser }), null);
      this.addResult('Preferences Command Execution', result.success, result.message);
    } catch (error) {
      this.addResult('Preferences Command Execution', false, error.message);
    }
  }

  async testDangerousCommands(coreModule, logMonitor) {
    if (!coreModule || !logMonitor) return;
    
    this.log('⚠️ Testing potentially dangerous commands with log monitoring...');
    
    // Test diagnostics command (this runs actual tests)
    try {
      this.log('Running diagnostics - checking for new errors...');
      const diagnosticsCmd = coreModule.getCommand('diagnostics');
      
      const result = await diagnosticsCmd.execute('{"type": "current"}', null);
      
      // Check for new errors in logs
      const newErrors = logMonitor.checkNewErrors();
      if (newErrors.length > 0) {
        this.addResult('Diagnostics Error Check', false, `${newErrors.length} new errors in logs`);
        newErrors.forEach((error, i) => {
          this.log(`   Error ${i + 1}: ${error.data?.error || error.raw || 'Unknown error'}`);
        });
      } else {
        this.addResult('Diagnostics Error Check', true, 'No new errors in logs');
      }
      
      this.addResult('Diagnostics Command Execution', result.success, result.message);
      
    } catch (error) {
      this.addResult('Diagnostics Command Execution', false, error.message);
    }
  }

  async runFullTest() {
    this.log('🧪 SAFE INTEGRATION TEST - Full Command System');
    this.log('=' * 60);
    
    // Start log monitoring
    const logMonitor = this.monitorLogs();
    
    // Test command loading
    const coreModule = await this.testCommandLoading();
    
    // Test safe commands
    await this.testSafeCommands(coreModule);
    
    // Test dangerous commands with monitoring
    await this.testDangerousCommands(coreModule, logMonitor);
    
    // Final results
    this.log('\n' + '=' * 60);
    this.log('🎯 INTEGRATION TEST RESULTS:');
    this.log('=' * 60);
    
    const passed = this.results.filter(r => r.success).length;
    const total = this.results.length;
    
    this.results.forEach(result => {
      const status = result.success ? '✅' : '❌';
      this.log(`${status} ${result.test}: ${result.details}`);
    });
    
    this.log(`\n🏁 FINAL: ${passed}/${total} tests passed`);
    
    if (passed === total) {
      this.log('🎉 ALL INTEGRATION TESTS PASSED - SYSTEM STABLE');
      return true;
    } else {
      this.log('⚠️ SOME TESTS FAILED - INVESTIGATE BEFORE DEPLOYING');
      return false;
    }
  }
}

// Run the test
async function main() {
  const test = new SafeIntegrationTest();
  const success = await test.runFullTest();
  process.exit(success ? 0 : 1);
}

if (require.main === module) {
  main().catch(error => {
    console.error('❌ Test failed:', error);
    process.exit(1);
  });
}

module.exports = SafeIntegrationTest;