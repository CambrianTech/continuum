#!/usr/bin/env tsx
/**
 * Safe Dashboard Launch for npm start
 * 
 * Handles the user requirement:
 * 1. Auto-launch dashboard when npm start completes
 * 2. Allow safe exit with Ctrl+B+D without killing system
 * 3. Provide clear exit instructions
 */

import { spawn } from 'child_process';

async function safeDashboardLaunch(): Promise<void> {
  console.log('🎯 Setting up JTAG dashboard in background...');
  
  // Wait for tmux session to exist (system fully started)
  let attempts = 0;
  const maxAttempts = 30;
  
  while (attempts < maxAttempts) {
    try {
      const checkSession = spawn('tmux', ['has-session', '-t', 'jtag-test'], { stdio: 'ignore' });
      
      const sessionExists = await new Promise<boolean>((resolve) => {
        checkSession.on('close', (code) => resolve(code === 0));
      });
      
      if (sessionExists) {
        console.log('✅ JTAG system ready');
        break;
      }
      
      console.log(`⏳ Waiting for system startup... (${attempts + 1}/${maxAttempts})`);
      await new Promise(resolve => setTimeout(resolve, 1000));
      attempts++;
      
    } catch {
      attempts++;
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }
  
  if (attempts >= maxAttempts) {
    console.log('❌ System startup timeout');
    console.log('💡 Try manually: npm run logs:attach');
    return;
  }
  
  console.log('📋 JTAG Dashboard ready in background tmux session');
  console.log('');
  console.log('💡 TO ACCESS DASHBOARD:');
  console.log('   tmux attach-session -t jtag-test');
  console.log('   npm run logs:attach (quick access)');
  console.log('');
  console.log('💡 TO EXIT DASHBOARD SAFELY:');
  console.log('   • Ctrl+B then D (tmux detach)');
  console.log('   • Ctrl+A then D (if prefix is Ctrl+A)');
  console.log('');
  console.log('🎯 LIVE NPM LOGS (Ctrl+C to exit npm start):');
  console.log('┌─────────────────────────┐');
  console.log('│     📦 NPM LOGS         │');
  console.log('└─────────────────────────┘');
  console.log('');
  
  // Now tail the npm logs directly (Ctrl+C exits npm start safely)
  const fs = require('fs');
  const logFile = '.continuum/jtag/system/logs/npm-start.log';
  
  try {
    // Make sure log file exists
    if (!fs.existsSync(logFile)) {
      console.log(`⚠️  Log file not found: ${logFile}`);
      console.log('💡 System may still be starting up');
      return;
    }
    
    // Tail logs directly - Ctrl+C will exit npm start process safely
    const tail = spawn('tail', ['-f', logFile], {
      stdio: 'inherit'
    });
    
    // When tail exits (via Ctrl+C), npm start exits cleanly
    tail.on('close', () => {
      console.log('');
      console.log('👋 npm start complete');
    });
    
  } catch (error) {
    console.error('❌ Log tail failed:', error instanceof Error ? error.message : String(error));
    console.log('💡 Manual logs: tail -f .continuum/jtag/system/logs/npm-start.log');
  }
}

if (require.main === module) {
  safeDashboardLaunch().catch(console.error);
}

export { safeDashboardLaunch };