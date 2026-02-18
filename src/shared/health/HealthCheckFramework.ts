/**
 * Modular Health Check Framework - Reusable system validation
 * 
 * Abstracts the health check pattern from the improved system detector
 * into a reusable framework for any system validation needs.
 */

import { execAsync, ProcessResult } from '../utils/ProcessUtils';

export interface HealthCheckResult {
  name: string;
  success: boolean;
  details: string;
  duration?: number;
  metadata?: Record<string, any>;
}

export interface HealthCheck {
  name: string;
  description?: string;
  timeout?: number;
  check: () => Promise<HealthCheckResult>;
}

export interface HealthSuite {
  name: string;
  description?: string;
  checks: HealthCheck[];
  onComplete?: (results: HealthCheckResult[]) => void;
}

export class HealthCheckRunner {
  private suites: Map<string, HealthSuite> = new Map();

  /**
   * Register a health check suite
   */
  registerSuite(suiteName: string, suite: HealthSuite): void {
    this.suites.set(suiteName, suite);
  }

  /**
   * Run all checks in a suite
   */
  async runSuite(suiteName: string): Promise<HealthCheckResult[]> {
    const suite = this.suites.get(suiteName);
    if (!suite) {
      throw new Error(`Health check suite '${suiteName}' not found`);
    }

    console.log(`🧪 Running Health Suite: ${suite.name}`);
    if (suite.description) {
      console.log(`   ${suite.description}`);
    }
    console.log();

    const results: HealthCheckResult[] = [];

    for (const check of suite.checks) {
      const startTime = Date.now();
      
      try {
        const result = await Promise.race([
          check.check(),
          this.createTimeoutPromise(check.timeout || 10000, check.name)
        ]);

        result.duration = Date.now() - startTime;
        results.push(result);
        
        const status = result.success ? '✅ PASS' : '❌ FAIL';
        console.log(`${status}: ${check.name}`);
        console.log(`   ${result.details}`);
        if (result.duration) {
          console.log(`   Duration: ${result.duration}ms`);
        }
        
      } catch (error: any) {
        const failedResult: HealthCheckResult = {
          name: check.name,
          success: false,
          details: `Exception: ${error.message}`,
          duration: Date.now() - startTime
        };
        results.push(failedResult);
        console.log(`❌ FAIL: ${check.name}`);
        console.log(`   Exception: ${error.message}`);
      }
      console.log();
    }

    if (suite.onComplete) {
      suite.onComplete(results);
    }

    return results;
  }

  /**
   * Run specific checks by name
   */
  async runChecks(suiteName: string, checkNames: string[]): Promise<HealthCheckResult[]> {
    const suite = this.suites.get(suiteName);
    if (!suite) {
      throw new Error(`Health check suite '${suiteName}' not found`);
    }

    const targetChecks = suite.checks.filter(check => checkNames.includes(check.name));
    const tempSuite: HealthSuite = {
      name: `${suite.name} (filtered)`,
      checks: targetChecks
    };

    this.suites.set('temp', tempSuite);
    const results = await this.runSuite('temp');
    this.suites.delete('temp');
    
    return results;
  }

  /**
   * Get summary statistics
   */
  getSummary(results: HealthCheckResult[]): {
    total: number;
    passed: number;
    failed: number;
    successRate: number;
    totalDuration: number;
  } {
    const passed = results.filter(r => r.success).length;
    const failed = results.length - passed;
    const totalDuration = results.reduce((sum, r) => sum + (r.duration || 0), 0);

    return {
      total: results.length,
      passed,
      failed,
      successRate: results.length > 0 ? (passed / results.length) * 100 : 0,
      totalDuration
    };
  }

  private async createTimeoutPromise(timeoutMs: number, checkName: string): Promise<HealthCheckResult> {
    return new Promise((_, reject) => {
      setTimeout(() => {
        reject(new Error(`Health check '${checkName}' timed out after ${timeoutMs}ms`));
      }, timeoutMs);
    });
  }
}

/**
 * Built-in health checks for common system validation
 */
export class SystemHealthChecks {
  static httpEndpoint(url: string, expectedStatus: number = 200): HealthCheck {
    return {
      name: `HTTP ${url}`,
      description: `Test HTTP endpoint responds with ${expectedStatus}`,
      check: async (): Promise<HealthCheckResult> => {
        const result = await execAsync(`curl -s -w "%{http_code}" -o /dev/null "${url}"`);
        if (result.success) {
          const statusCode = parseInt(result.stdout.trim());
          if (statusCode === expectedStatus) {
            return {
              name: `HTTP ${url}`,
              success: true,
              details: `HTTP ${statusCode} - Endpoint responding correctly`,
              metadata: { statusCode, url }
            };
          } else {
            return {
              name: `HTTP ${url}`,
              success: false,
              details: `HTTP ${statusCode} - Expected ${expectedStatus}`,
              metadata: { statusCode, expectedStatus, url }
            };
          }
        } else {
          return {
            name: `HTTP ${url}`,
            success: false,
            details: `Connection failed: ${result.stderr}`,
            metadata: { url, error: result.stderr }
          };
        }
      }
    };
  }

  static portOpen(port: number, host: string = 'localhost'): HealthCheck {
    return {
      name: `Port ${port}`,
      description: `Test if port ${port} is accepting connections`,
      check: async (): Promise<HealthCheckResult> => {
        const result = await execAsync(`nc -z ${host} ${port}`);
        return {
          name: `Port ${port}`,
          success: result.success,
          details: result.success 
            ? `Port ${port} accepting connections`
            : `Port ${port} not accepting connections`,
          metadata: { port, host }
        };
      }
    };
  }

  static processRunning(processName: string): HealthCheck {
    return {
      name: `Process ${processName}`,
      description: `Test if process ${processName} is running`,
      check: async (): Promise<HealthCheckResult> => {
        const result = await execAsync(`pgrep -f "${processName}"`);
        if (result.success && result.stdout.trim()) {
          const pids = result.stdout.trim().split('\n');
          return {
            name: `Process ${processName}`,
            success: true,
            details: `Process running with ${pids.length} instance(s)`,
            metadata: { processName, pids }
          };
        } else {
          return {
            name: `Process ${processName}`,
            success: false,
            details: `Process ${processName} not found`,
            metadata: { processName }
          };
        }
      }
    };
  }

  static fileExists(filePath: string): HealthCheck {
    return {
      name: `File ${filePath}`,
      description: `Test if file exists`,
      check: async (): Promise<HealthCheckResult> => {
        const result = await execAsync(`test -f "${filePath}"`);
        return {
          name: `File ${filePath}`,
          success: result.success,
          details: result.success 
            ? `File exists: ${filePath}`
            : `File not found: ${filePath}`,
          metadata: { filePath }
        };
      }
    };
  }

  static directoryExists(dirPath: string): HealthCheck {
    return {
      name: `Directory ${dirPath}`,
      description: `Test if directory exists`,
      check: async (): Promise<HealthCheckResult> => {
        const result = await execAsync(`test -d "${dirPath}"`);
        return {
          name: `Directory ${dirPath}`,
          success: result.success,
          details: result.success 
            ? `Directory exists: ${dirPath}`
            : `Directory not found: ${dirPath}`,
          metadata: { dirPath }
        };
      }
    };
  }
}