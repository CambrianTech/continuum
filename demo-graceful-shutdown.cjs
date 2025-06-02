#!/usr/bin/env node
/**
 * Demo Script for Graceful Shutdown and Port Conflict Resolution
 * 
 * This script demonstrates the new features:
 * 1. Automatic port conflict resolution
 * 2. Graceful shutdown handling
 * 3. Stay-alive mode
 * 4. CLI commands for process management
 */

const { spawn } = require('child_process');
const path = require('path');

const CONTINUUM_SCRIPT = path.join(__dirname, 'continuum.cjs');

function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function runCommand(command, args = []) {
  return new Promise((resolve, reject) => {
    console.log(`\n💻 Running: node continuum.cjs ${command} ${args.join(' ')}`);
    const proc = spawn('node', [CONTINUUM_SCRIPT, command, ...args], {
      stdio: 'inherit'
    });
    
    proc.on('exit', (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Command failed with exit code ${code}`));
      }
    });
  });
}

async function demo() {
  console.log('🚀 Continuum Graceful Shutdown & Port Conflict Resolution Demo');
  console.log('==============================================================');
  
  try {
    console.log('\n📋 Step 1: Check initial status (should be no running instances)');
    await runCommand('status');
    
    console.log('\n📋 Step 2: Start first instance on port 5570');
    const proc1 = spawn('node', [CONTINUUM_SCRIPT, 'start', '--port', '5570'], {
      stdio: 'pipe'
    });
    
    // Wait for startup
    await sleep(4000);
    
    console.log('\n📋 Step 3: Check status (should show running instance)');
    await runCommand('status');
    
    console.log('\n📋 Step 4: Try to start second instance with stay-alive mode');
    console.log('   (should detect existing instance and exit gracefully)');
    await runCommand('start', ['--stay-alive', '--port', '5570']);
    
    console.log('\n📋 Step 5: Start second instance without stay-alive');
    console.log('   (should shutdown existing instance and take over)');
    const proc2 = spawn('node', [CONTINUUM_SCRIPT, 'start', '--port', '5570'], {
      stdio: 'pipe'
    });
    
    // Wait for takeover
    await sleep(4000);
    
    console.log('\n📋 Step 6: Check status again (should show new PID)');
    await runCommand('status');
    
    console.log('\n📋 Step 7: Gracefully shutdown using CLI command');
    await runCommand('stop');
    
    console.log('\n📋 Step 8: Final status check (should be no running instances)');
    await runCommand('status');
    
    console.log('\n✅ Demo completed successfully!');
    console.log('\n🎉 Key Features Demonstrated:');
    console.log('   ✅ Automatic port conflict detection and resolution');
    console.log('   ✅ Graceful shutdown with SIGTERM handling');
    console.log('   ✅ Stay-alive mode to preserve existing instances');
    console.log('   ✅ CLI commands for process management (start, stop, status)');
    console.log('   ✅ PID file management and stale process cleanup');
    
  } catch (error) {
    console.error('\n❌ Demo failed:', error.message);
    
    // Cleanup - try to stop any running instances
    try {
      await runCommand('stop');
    } catch (cleanupError) {
      console.log('   (Cleanup attempted)');
    }
    
    process.exit(1);
  }
}

// Run demo if this script is executed directly
if (require.main === module) {
  demo().catch(error => {
    console.error('Demo runner failed:', error);
    process.exit(1);
  });
}

module.exports = { demo };