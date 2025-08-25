#!/usr/bin/env tsx
/**
 * Test System Starter - Launches system in tmux and exits immediately
 * 
 * This is specifically for the test runner which needs the system to start
 * in the background but the launcher command to exit so tests can proceed.
 */

import { spawn } from 'child_process';
import { TmuxSessionManager } from '../system/shared/TmuxSessionManager';

async function startSystemForTesting(): Promise<void> {
  console.log('🧪 TEST SYSTEM STARTER: Launching JTAG system in tmux background...');
  
  // Kill any existing session first
  const sessionName = TmuxSessionManager.getSessionName();
  console.log(`🧹 Cleaning up any existing session: ${sessionName}`);
  
  try {
    await new Promise<void>((resolve) => {
      const killTmux = spawn('tmux', ['kill-session', '-t', sessionName], { stdio: 'ignore' });
      killTmux.on('close', () => resolve());
    });
    // Wait for cleanup
    await new Promise(resolve => setTimeout(resolve, 2000));
  } catch {
    // Ignore - session might not exist
  }
  
  // Create new tmux session with the system
  console.log(`🚀 Creating tmux session: ${sessionName}`);
  
  return new Promise<void>((resolve, reject) => {
    const tmux = spawn('tmux', [
      'new-session', 
      '-d', 
      '-s', 
      sessionName,
      'npm', 
      'run', 
      'start:direct'
    ], {
      stdio: 'pipe'
    });
    
    tmux.on('exit', (code) => {
      if (code === 0) {
        console.log('✅ TEST SYSTEM STARTER: Tmux session created successfully');
        console.log('🎯 System starting in background - ready for test execution');
        resolve();
      } else {
        console.error(`❌ TEST SYSTEM STARTER: Failed to create tmux session (exit code: ${code})`);
        reject(new Error(`Tmux creation failed with code ${code}`));
      }
    });
    
    tmux.on('error', (error) => {
      console.error(`❌ TEST SYSTEM STARTER: Error creating tmux session: ${error.message}`);
      reject(error);
    });
  });
}

// Run if called directly
if (require.main === module) {
  startSystemForTesting()
    .then(() => {
      console.log('🎉 TEST SYSTEM STARTER: Complete - system launched, exiting');
      process.exit(0);
    })
    .catch((error) => {
      console.error('💥 TEST SYSTEM STARTER: Failed:', error.message);
      process.exit(1);
    });
}