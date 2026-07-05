/**
 * Test Bench Client - Coordinates with running test-bench
 * 
 * This client connects to the test-bench server and provides utilities
 * for cross-environment testing without shutting down the system.
 */

import * as http from 'http';

export interface TestBenchConnection {
  serverPort: number;
  jtagPort: number;
  connected: boolean;
  environment: 'browser' | 'server';
}

export interface BrowserExecutionResult {
  success: boolean;
  result?: any;
  error?: string;
  logs?: string[];
}

export class TestBenchClient {
  private readonly TEST_BENCH_PORT = 9002;
  private readonly JTAG_PORT = 9001;
  private connection: TestBenchConnection | null = null;

  /**
   * Connect to running test-bench (doesn't start it - expects it to be running)
   */
  async connect(): Promise<TestBenchConnection> {
    try {
      // Verify test-bench server is running
      await this.httpGet(`http://localhost:${this.TEST_BENCH_PORT}`);
      
      this.connection = {
        serverPort: this.TEST_BENCH_PORT,
        jtagPort: this.JTAG_PORT,
        connected: true,
        environment: 'server' // We're connecting from server-side test
      };
      
      return this.connection;
    } catch (error: any) {
      throw new Error(`Test-bench not running on port ${this.TEST_BENCH_PORT}. Make sure 'npm start' is running.`);
    }
  }

  /**
   * Execute JavaScript code in the browser context via test-bench
   */
  async executeInBrowser(code: string): Promise<BrowserExecutionResult> {
    if (!this.connection) {
      throw new Error('Not connected to test-bench. Call connect() first.');
    }

    try {
      // Send code to test-bench for browser execution
      const response = await this.httpPost(
        `http://localhost:${this.TEST_BENCH_PORT}/execute-in-browser`,
        { code }
      );

      return JSON.parse(response.data);
    } catch (error: any) {
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Get current JTAG system status from test-bench
   */
  async getJTAGStatus(): Promise<any> {
    if (!this.connection) {
      throw new Error('Not connected to test-bench. Call connect() first.');
    }

    try {
      const response = await this.httpGet(`http://localhost:${this.TEST_BENCH_PORT}/jtag-status`);
      return JSON.parse(response.data);
    } catch (error: any) {
      throw new Error(`Failed to get JTAG status: ${error.message}`);
    }
  }

  /**
   * Check if log files exist and contain expected content
   */
  async verifyLogFiles(testId: string): Promise<boolean> {
    if (!this.connection) {
      throw new Error('Not connected to test-bench. Call connect() first.');
    }

    try {
      const response = await this.httpPost(
        `http://localhost:${this.TEST_BENCH_PORT}/verify-logs`,
        { testId }
      );

      const result = JSON.parse(response.data);
      return result.success;
    } catch (error: any) {
      return false;
    }
  }

  /**
   * Check if screenshot files exist
   */
  async verifyScreenshots(filename: string): Promise<boolean> {
    if (!this.connection) {
      throw new Error('Not connected to test-bench. Call connect() first.');
    }

    try {
      const response = await this.httpPost(
        `http://localhost:${this.TEST_BENCH_PORT}/verify-screenshot`,
        { filename }
      );

      const result = JSON.parse(response.data);
      return result.exists;
    } catch (error: any) {
      return false;
    }
  }

  /**
   * Test WebSocket connection health
   */
  async testWebSocketHealth(): Promise<{connected: boolean, latency?: number}> {
    if (!this.connection) {
      throw new Error('Not connected to test-bench. Call connect() first.');
    }

    try {
      const startTime = Date.now();
      const response = await this.httpGet(`http://localhost:${this.TEST_BENCH_PORT}/websocket-health`);
      const latency = Date.now() - startTime;

      const result = JSON.parse(response.data);
      return {
        connected: result.connected,
        latency
      };
    } catch (error: any) {
      return { connected: false };
    }
  }

  /**
   * Send a test message and verify it appears in logs
   */
  async testCrossEnvironmentLogging(testMessage: string): Promise<boolean> {
    if (!this.connection) {
      throw new Error('Not connected to test-bench. Call connect() first.');
    }

    const testId = `test_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
    const fullMessage = `${testMessage} [${testId}]`;

    try {
      // Execute console.log in browser
      const browserResult = await this.executeInBrowser(`
        console.log('${fullMessage}');
        return { testId: '${testId}', message: '${fullMessage}' };
      `);

      if (!browserResult.success) {
        return false;
      }

      // Wait a bit for log transport
      await this.sleep(1000);

      // Verify message appeared in server logs
      return await this.verifyLogFiles(testId);
    } catch (error: any) {
      return false;
    }
  }

  private async httpGet(url: string): Promise<{statusCode: number, data: string}> {
    return new Promise((resolve, reject) => {
      const request = http.get(url, (response) => {
        let data = '';
        response.on('data', (chunk) => data += chunk);
        response.on('end', () => {
          resolve({
            statusCode: response.statusCode || 0,
            data
          });
        });
      });
      
      request.on('error', reject);
      request.setTimeout(10000, () => {
        request.destroy();
        reject(new Error('Request timeout'));
      });
    });
  }

  private async httpPost(url: string, body: any): Promise<{statusCode: number, data: string}> {
    return new Promise((resolve, reject) => {
      const postData = JSON.stringify(body);
      const options = {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Content-Length': Buffer.byteLength(postData)
        }
      };

      const request = http.request(url, options, (response) => {
        let data = '';
        response.on('data', (chunk) => data += chunk);
        response.on('end', () => {
          resolve({
            statusCode: response.statusCode || 0,
            data
          });
        });
      });

      request.on('error', reject);
      request.setTimeout(10000, () => {
        request.destroy();
        reject(new Error('Request timeout'));
      });

      request.write(postData);
      request.end();
    });
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  disconnect(): void {
    this.connection = null;
  }
}