#!/usr/bin/env node

console.log('=== Testing Console Daemon Interception ===');

const originalLog = console.log;
const originalError = console.error;

async function testConsoleDaemon() {
  try {
    console.log('STEP 1: Before daemon system initialization');
    
    // Import the daemon system
    const { createJTAGDaemonSystem } = require('./daemons/index');
    
    console.log('STEP 2: Creating daemon system...');
    const daemonSystem = createJTAGDaemonSystem('universal');
    
    console.log('STEP 3: Registering daemons...');
    await daemonSystem.registerDaemons();
    
    console.log('STEP 4: Testing console interception...');
    console.log('AFTER SETUP: This should be intercepted by ConsoleDaemon');
    console.error('AFTER SETUP: This error should be intercepted');
    console.warn('AFTER SETUP: This warning should be intercepted');
    
    // Check if console was actually modified
    const logModified = console.log !== originalLog;
    const errorModified = console.error !== originalError;
    
    console.log('Console.log was modified:', logModified);
    console.log('Console.error was modified:', errorModified);
    
    if (logModified || errorModified) {
      console.log('✅ SUCCESS: Console interception is working!');
    } else {
      console.log('❌ FAILURE: Console was not intercepted');
    }
    
    console.log('STEP 5: Checking if log files were created...');
    const fs = require('fs');
    const path = require('path');
    
    const logDir = '.continuum/jtag/logs';
    if (fs.existsSync(logDir)) {
      const files = fs.readdirSync(logDir);
      console.log('Log files found:', files);
    } else {
      console.log('No log directory found');
    }
    
  } catch (error) {
    console.error('Test failed:', error.message);
    console.error('Stack:', error.stack);
  }
}

testConsoleDaemon().then(() => {
  console.log('=== Test Completed ===');
}).catch(error => {
  console.error('Async error:', error.message);
});