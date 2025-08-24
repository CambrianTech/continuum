// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command - Run existing tests
 * 
 * Usage:
 *   ./jtag test                           # Run npm test (full suite)
 *   ./jtag test tests/something.test.ts   # Run specific test file
 */

import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class TestCommand {
  constructor(private context: any, private subpath: string, private commander: any) {}
  
  async execute(params: any = {}) {
    const startTime = Date.now();
    
    // Determine what to run
    let command: string;
    if (params.file || (params._ && params._[0])) {
      const testFile = params.file ?? params._[0];
      command = `npx tsx ${testFile}`;
      console.log(`🧪 Running test file: ${testFile}`);
    } else {
      command = 'npm test';
      console.log('🧪 Running full test suite...');
    }
    
    try {
      const timeout = params.timeout ?? 300000; // 5 minutes
      console.log(`⚡ Executing: ${command}`);
      
      const { stdout, stderr } = await execAsync(command, {
        timeout,
        maxBuffer: 1024 * 1024 * 10
      });
      
      const duration = Date.now() - startTime;
      const output = stdout + (stderr ? '\n' + stderr : '');
      
      console.log(`✅ Test completed in ${duration}ms`);
      
      return {
        success: true,
        output,
        duration,
        command
      };
      
    } catch (error: any) {
      const duration = Date.now() - startTime;
      const output = (error.stdout ?? '') + (error.stderr ?? '');
      
      if (error.code === 'ETIMEDOUT') {
        console.error(`❌ TIMEOUT: Test failed to complete within ${params.timeout ?? 300000}ms - test cancelled`);
      } else {
        console.error(`❌ Test failed (${duration}ms)`);
      }
      
      return {
        success: false,
        output,
        duration,
        command
      };
    }
  }
}