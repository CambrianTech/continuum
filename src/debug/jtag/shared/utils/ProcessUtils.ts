/**
 * Shared Process Utilities - DRY principle for async exec operations
 * 
 * Eliminates the repeated pattern of:
 * import { exec } from 'child_process';
 * import { promisify } from 'util';
 * const execAsync = promisify(exec);
 */

import { exec, ExecOptions } from 'child_process';
import { promisify } from 'util';

const baseExecAsync = promisify(exec);

export interface ProcessResult {
  stdout: string;
  stderr: string;
  success: boolean;
  exitCode?: number;
  duration?: number;
}

/**
 * Enhanced async exec with better error handling and timing
 */
export async function execAsync(
  command: string, 
  options: ExecOptions = {}
): Promise<ProcessResult> {
  const startTime = Date.now();
  
  try {
    const result = await baseExecAsync(command, {
      timeout: 30000, // 30s default timeout
      ...options
    });
    
    return {
      stdout: result.stdout,
      stderr: result.stderr,
      success: true,
      duration: Date.now() - startTime
    };
  } catch (error: any) {
    return {
      stdout: error.stdout || '',
      stderr: error.stderr || error.message,
      success: false,
      exitCode: error.code,
      duration: Date.now() - startTime
    };
  }
}

/**
 * Quick exec for commands where you only need stdout success
 */
export async function execQuick(command: string): Promise<string> {
  const result = await execAsync(command);
  if (!result.success) {
    throw new Error(`Command failed: ${command}\n${result.stderr}`);
  }
  return result.stdout.trim();
}

/**
 * Exec with retry logic for flaky commands
 */
export async function execWithRetry(
  command: string, 
  retries: number = 3,
  delayMs: number = 1000
): Promise<ProcessResult> {
  let lastResult: ProcessResult | null = null;
  
  for (let attempt = 1; attempt <= retries; attempt++) {
    lastResult = await execAsync(command);
    
    if (lastResult.success) {
      return lastResult;
    }
    
    if (attempt < retries) {
      await new Promise(resolve => setTimeout(resolve, delayMs));
    }
  }
  
  return lastResult!;
}

/**
 * Test if a command exists in PATH
 */
export async function commandExists(command: string): Promise<boolean> {
  const result = await execAsync(`which ${command}`);
  return result.success;
}

/**
 * Get process info with better typing
 */
export interface ProcessInfo {
  pid: number;
  name: string;
  cpu: number;
  memory: number;
}

export async function getProcessesByName(name: string): Promise<ProcessInfo[]> {
  const result = await execAsync(`ps aux | grep -v grep | grep "${name}"`);
  if (!result.success) return [];
  
  return result.stdout
    .split('\n')
    .filter(line => line.trim())
    .map(line => {
      const parts = line.trim().split(/\s+/);
      return {
        pid: parseInt(parts[1]) || 0,
        name: parts[10] || '',
        cpu: parseFloat(parts[2]) || 0,
        memory: parseFloat(parts[3]) || 0
      };
    });
}