#!/usr/bin/env tsx
/**
 * Intelligent Test Runner - Handles cross-example switching and versioning
 * 
 * PROBLEM SOLVED: When you run widget-ui, then npm test (test-bench), 
 * ports conflict because different examples use same ports.
 * 
 * INTELLIGENCE: 
 * 1. Force cleanup ALL existing systems (any example)
 * 2. Handle versioning conflicts automatically 
 * 3. Wait for ports to be released properly
 * 4. Start clean test-bench environment
 */

import { spawn } from 'child_process';

async function runIntelligentTest(): Promise<boolean> {
  console.log('🧠 INTELLIGENT TEST RUNNER: Handling cross-example switching');
  console.log('📋 Solving widget-ui→test-bench port conflicts automatically');
  
  // INTELLIGENCE: Step 1 - Force cleanup ALL existing systems
  console.log('\n🧹 STEP 1: Force cleanup of all existing JTAG systems (any example)');
  try {
    await new Promise<void>((resolve) => {
      const cleanup = spawn('npm', ['run', 'system:stop'], {
        stdio: 'inherit',
        shell: true
      });
      
      cleanup.on('exit', (code) => {
        console.log(`✅ System cleanup completed (exit code: ${code})`);
        resolve();
      });
      
      cleanup.on('error', (error) => {
        console.warn(`⚠️ Cleanup error (continuing anyway): ${error.message}`);
        resolve(); // Continue even if cleanup fails
      });
    });
    
    // INTELLIGENCE: Extra wait for ports to be fully released
    console.log('⏳ Waiting for ports to be fully released...');
    await new Promise(resolve => setTimeout(resolve, 3000));
    
  } catch (error) {
    console.warn(`⚠️ Cleanup failed (continuing): ${error}`);
  }
  
  // INTELLIGENCE: Step 2 - Kill any lingering processes on our ports
  console.log('\n🔫 STEP 2: Kill any lingering processes on ports 9001, 9002');
  try {
    for (const port of [9001, 9002]) {
      await new Promise<void>((resolve) => {
        // Use lsof to find and kill processes on specific ports
        const killCmd = `lsof -ti :${port} | xargs kill -9 2>/dev/null || true`;
        const killPort = spawn('bash', ['-c', killCmd], {
          stdio: 'pipe', // Don't show output for these
          shell: false
        });
        
        killPort.on('exit', () => resolve());
        killPort.on('error', () => resolve()); // Continue even if fails
      });
    }
    console.log('✅ Port cleanup completed');
  } catch (error) {
    console.warn(`⚠️ Port cleanup failed (continuing): ${error}`);
  }
  
  // INTELLIGENCE: Step 3 - Start test system with smart environment
  console.log('\n🚀 STEP 3: Starting test-bench system with intelligent configuration');
  
  return new Promise<boolean>((resolve) => {
    const testProcess = spawn('npm', ['run', 'test:start-and-test'], {
      stdio: 'inherit',
      shell: true,
      env: {
        ...process.env,
        JTAG_ACTIVE_EXAMPLE: 'test-bench',     // Force test-bench example
        JTAG_FORCE_RESTART: 'true',           // Force restart to handle version conflicts
        JTAG_IGNORE_EXISTING: 'true'         // Ignore any existing systems
      }
    });
    
    // Handle interruption
    process.on('SIGINT', () => {
      console.log('\n🛑 Intelligent test interrupted - cleaning up...');
      testProcess.kill('SIGINT');
      setTimeout(() => {
        process.exit(130);
      }, 1000);
    });
    
    testProcess.on('exit', (code) => {
      const success = code === 0;
      if (success) {
        console.log('\n🎉 Intelligent test execution completed successfully!');
        console.log('🧠 Cross-example switching handled automatically');
      } else {
        console.log(`\n❌ Intelligent test failed with exit code: ${code}`);
        console.log('💡 Try running again - port conflicts may need more cleanup time');
      }
      resolve(success);
    });
    
    testProcess.on('error', (error) => {
      console.error(`\n💥 Intelligent test execution error: ${error.message}`);
      resolve(false);
    });
  });
}

// Main execution
async function main() {
  try {
    const success = await runIntelligentTest();
    process.exit(success ? 0 : 1);
  } catch (error: any) {
    console.error('\n💥 Intelligent test runner failed:', error.message);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}