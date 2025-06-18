/**
 * DiagnosticsCommand - Built-in system testing and diagnostics
 * Runs comprehensive system tests with isolated directories and fresh logs
 */

const BaseCommand = require('../../BaseCommand.cjs');
const { spawn } = require('child_process');
const path = require('path');

class DiagnosticsCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'diagnostics',
      description: 'Run built-in system tests with isolated directories and fresh logs',
      icon: '🧪',
      parameters: {
        type: { 
          type: 'string', 
          required: false, 
          description: 'Test type: isolated, screenshot, fresh-logs',
          default: 'isolated'
        },
        verbose: {
          type: 'boolean',
          required: false,
          description: 'Show detailed test output',
          default: true
        }
      },
      examples: [
        'diagnostics',
        'diagnostics --type screenshot',
        'diagnostics --type fresh-logs'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    const testType = options.type || 'isolated';
    
    console.log('🧪 CONTINUUM DIAGNOSTICS');
    console.log('=' * 50);
    
    try {
      // Determine test script based on type
      let testScript = 'isolated_test_suite.py';
      if (testType === 'screenshot') {
        testScript = 'fresh_log_test.py';
      } else if (testType === 'fresh-logs') {
        testScript = 'fresh_log_test.py';
      } else if (testType === 'current') {
        testScript = 'current_system_test.py';
      } else if (testType === 'unique') {
        testScript = 'unique_screenshot_test.py';
      }
      
      const pythonClientDir = path.join(process.cwd(), 'python-client');
      const testPath = path.join(pythonClientDir, testScript);
      
      console.log(`📝 Running: ${testScript}`);
      console.log(`📁 Directory: ${pythonClientDir}`);
      console.log('🔒 Tests use isolated subdirectories and fresh logs');
      console.log('✨ No false confidence from old files!');
      console.log('');
      
      const success = await this.runTestProcess(testPath, pythonClientDir);
      
      if (success) {
        console.log('\n🎉 Diagnostics completed successfully!');
        return this.createSuccessResult({ testType, script: testScript }, 'Diagnostics passed');
      } else {
        console.log('\n❌ Diagnostics failed!');
        return this.createErrorResult('Diagnostics failed', { testType, script: testScript });
      }
      
    } catch (error) {
      console.error('❌ Diagnostics error:', error);
      return this.createErrorResult('Diagnostics execution failed', error.message);
    }
  }
  
  static async runTestProcess(testPath, workingDir) {
    return new Promise((resolve, reject) => {
      const testProcess = spawn('python3', [testPath], {
        cwd: workingDir,
        stdio: 'inherit'
      });
      
      testProcess.on('close', (code) => {
        resolve(code === 0);
      });
      
      testProcess.on('error', (error) => {
        console.error('\n❌ Failed to run test process:', error);
        resolve(false);
      });
    });
  }
}

module.exports = DiagnosticsCommand;