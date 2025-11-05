/**
 * Modular Port Checking System with Strong Type Safety
 * 
 * Clean separation of port checking logic with multiple strategies
 * and consistent interface. No hard-coded dependencies.
 */

export type PortNumber = number & { readonly __brand: 'PortNumber' };
export type ProcessId = string & { readonly __brand: 'ProcessId' };
export type ErrorMessage = string & { readonly __brand: 'ErrorMessage' };

export const enum PortCheckMethod {
  LSOF = 'lsof',
  NETSTAT = 'netstat',
  SIGNAL = 'signal'
}

export const enum PortCheckResult {
  ACTIVE = 'active',
  INACTIVE = 'inactive',
  ERROR = 'error'
}

export interface PortStatus {
  readonly port: PortNumber;
  readonly isActive: boolean;
  readonly result: PortCheckResult;
  readonly method: PortCheckMethod;
  readonly process?: ProcessId;
  readonly error?: ErrorMessage;
  readonly timestamp: number;
}

export interface PortCheckStrategy {
  readonly name: string;
  readonly method: PortCheckMethod;
  checkPort(port: PortNumber): Promise<PortStatus>;
}

export interface PortCheckOptions {
  readonly timeoutMs?: number;
  readonly retries?: number;
  readonly strategy?: PortCheckMethod;
}

export interface PortWaitOptions extends PortCheckOptions {
  readonly pollIntervalMs?: number;
}

// Type guards
export function isValidPort(port: number): port is PortNumber {
  return Number.isInteger(port) && port > 0 && port <= 65535;
}

export function createPortNumber(port: number): PortNumber {
  if (!isValidPort(port)) {
    throw new Error(`Invalid port number: ${port}. Must be between 1 and 65535.`);
  }
  return port as PortNumber;
}

export function createProcessId(process: string): ProcessId {
  if (!process || process.trim().length === 0) {
    throw new Error('Process ID cannot be empty');
  }
  return process.trim() as ProcessId;
}

export function createErrorMessage(error: string): ErrorMessage {
  return error as ErrorMessage;
}

/**
 * Simple lsof-based port checking (most reliable)
 */
export class LsofPortChecker implements PortCheckStrategy {
  readonly name = 'lsof';
  readonly method = PortCheckMethod.LSOF;

  async checkPort(port: PortNumber): Promise<PortStatus> {
    const timestamp = Date.now();
    
    try {
      const { execSync } = await import('child_process');
      const result = execSync(`lsof -i :${port} -t`, { 
        encoding: 'utf8', 
        stdio: 'pipe',
        timeout: 5000 // 5 second timeout
      });
      
      const pids = result.trim();
      const isActive = pids.length > 0;
      
      return {
        port,
        isActive,
        result: isActive ? PortCheckResult.ACTIVE : PortCheckResult.INACTIVE,
        method: this.method,
        process: isActive ? createProcessId(pids) : undefined,
        timestamp
      };
    } catch (error: any) {
      return {
        port,
        isActive: false,
        result: PortCheckResult.ERROR,
        method: this.method,
        error: createErrorMessage(error.message || 'Unknown lsof error'),
        timestamp
      };
    }
  }
}

/**
 * Netstat-based port checking (fallback)
 */
export class NetstatPortChecker implements PortCheckStrategy {
  readonly name = 'netstat';
  readonly method = PortCheckMethod.NETSTAT;

  async checkPort(port: PortNumber): Promise<PortStatus> {
    const timestamp = Date.now();
    
    try {
      const { execSync } = await import('child_process');
      const result = execSync(`netstat -an | grep :${port}`, { 
        encoding: 'utf8', 
        stdio: 'pipe',
        timeout: 5000
      });
      
      const isActive = result.includes('LISTEN');
      
      return {
        port,
        isActive,
        result: isActive ? PortCheckResult.ACTIVE : PortCheckResult.INACTIVE,
        method: this.method,
        process: isActive ? createProcessId('listening') : undefined,
        timestamp
      };
    } catch (error: any) {
      return {
        port,
        isActive: false,
        result: PortCheckResult.ERROR,
        method: this.method,
        error: createErrorMessage(error.message || 'Unknown netstat error'),
        timestamp
      };
    }
  }
}

/**
 * Multi-strategy port checker with fallbacks
 */
export class PortChecker {
  private strategies: PortCheckStrategy[];

  constructor(strategies?: PortCheckStrategy[]) {
    this.strategies = strategies || [
      new LsofPortChecker(),
      new NetstatPortChecker()
    ];
  }

  /**
   * Check single port with primary strategy
   */
  async checkPort(port: PortNumber): Promise<PortStatus> {
    const primaryStrategy = this.strategies[0];
    if (!primaryStrategy) {
      throw new Error('No port checking strategies available');
    }
    return await primaryStrategy.checkPort(port);
  }

  /**
   * Check single port with all strategies (for debugging)
   */
  async checkPortAllStrategies(port: PortNumber): Promise<readonly PortStatus[]> {
    const results = await Promise.all(
      this.strategies.map(strategy => strategy.checkPort(port))
    );
    return results;
  }

  /**
   * Check multiple ports efficiently
   */
  async checkPorts(ports: readonly PortNumber[]): Promise<readonly PortStatus[]> {
    const results = await Promise.all(
      ports.map(port => this.checkPort(port))
    );
    return results;
  }

  /**
   * Wait for port to become active with timeout
   */
  async waitForPort(port: PortNumber, options: PortWaitOptions = {}): Promise<boolean> {
    const timeoutMs = options.timeoutMs ?? 30000;
    const pollIntervalMs = options.pollIntervalMs ?? 1000;
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
      const status = await this.checkPort(port);
      if (status.isActive && status.result === PortCheckResult.ACTIVE) {
        return true;
      }
      
      // Wait before retry
      await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
    }
    
    return false;
  }

  /**
   * Wait for multiple ports with timeout
   */
  async waitForPorts(ports: readonly PortNumber[], options: PortWaitOptions = {}): Promise<boolean> {
    const timeoutMs = options.timeoutMs ?? 30000;
    const pollIntervalMs = options.pollIntervalMs ?? 1000;
    const startTime = Date.now();
    
    while (Date.now() - startTime < timeoutMs) {
      const results = await this.checkPorts(ports);
      const allActive = results.every(result => 
        result.isActive && result.result === PortCheckResult.ACTIVE
      );
      
      if (allActive) {
        return true;
      }
      
      // Wait before retry
      await new Promise(resolve => setTimeout(resolve, pollIntervalMs));
    }
    
    return false;
  }

  /**
   * Convenience method to check port with number validation
   */
  async checkPortNumber(port: number): Promise<PortStatus> {
    const validatedPort = createPortNumber(port);
    return await this.checkPort(validatedPort);
  }
}