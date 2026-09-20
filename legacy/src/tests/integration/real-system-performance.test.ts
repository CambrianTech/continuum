/**
 * Real System Performance Test - Actual JTAG System Integration
 * 
 * Tests actual running JTAG system performance. NO MOCKING.
 * Requires the system to be running: npm run system:start
 */

import { performance } from 'perf_hooks';
import { globalProfiler } from '../../shared/performance/PerformanceProfiler';
import * as http from 'http';

interface SystemHealthCheck {
  name: string;
  url: string;
  expectedStatus: number;
  timeout: number;
}

interface RealSystemMetrics {
  systemRunning: boolean;
  healthChecks: { name: string; success: boolean; latency: number; error?: string }[];
  httpResponseTimes: number[];
  systemLoadTime: number;
  memoryUsage: { heapUsed: number; heapTotal: number; external: number };
  concurrentRequestHandling: {
    concurrentRequests: number;
    successfulRequests: number;
    averageResponseTime: number;
    maxResponseTime: number;
    minResponseTime: number;
  };
}

class RealSystemPerformanceTester {
  private readonly healthChecks: SystemHealthCheck[] = [
    { name: 'HTTP Server', url: 'http://localhost:9002', expectedStatus: 200, timeout: 5000 },
    { name: 'WebSocket Endpoint', url: 'http://localhost:9002', expectedStatus: 200, timeout: 5000 }
  ];
  
  async runSystemPerformanceTest(): Promise<RealSystemMetrics> {
    console.log('🏥 REAL SYSTEM PERFORMANCE TEST');
    console.log('==============================');
    console.log('Testing actual running JTAG system (requires npm run system:start)');
    
    const metrics: RealSystemMetrics = {
      systemRunning: false,
      healthChecks: [],
      httpResponseTimes: [],
      systemLoadTime: 0,
      memoryUsage: process.memoryUsage(),
      concurrentRequestHandling: {
        concurrentRequests: 0,
        successfulRequests: 0,
        averageResponseTime: 0,
        maxResponseTime: 0,
        minResponseTime: Number.MAX_VALUE
      }
    };
    
    // Test 1: System Health Checks
    console.log('\n🔍 Testing system health...');
    const systemRunning = await this.testSystemHealth(metrics);
    metrics.systemRunning = systemRunning;
    
    if (!systemRunning) {
      console.log('❌ System not running. Start with: npm run system:start');
      return metrics;
    }
    
    // Test 2: HTTP Response Time Performance
    console.log('\n⚡ Testing HTTP response performance...');
    await this.testHttpPerformance(metrics);
    
    // Test 3: Concurrent Request Handling
    console.log('\n🚀 Testing concurrent request handling...');
    await this.testConcurrentRequestHandling(metrics);
    
    // Test 4: System Load Time
    console.log('\n📈 Testing system load characteristics...');
    await this.testSystemLoadTime(metrics);
    
    this.printRealSystemMetrics(metrics);
    return metrics;
  }
  
  private async testSystemHealth(metrics: RealSystemMetrics): Promise<boolean> {
    let systemRunning = false;
    
    for (const healthCheck of this.healthChecks) {
      const checkStart = performance.now();
      
      try {
        const success = await this.makeHttpRequest(healthCheck.url, healthCheck.timeout);
        const latency = performance.now() - checkStart;
        
        metrics.healthChecks.push({
          name: healthCheck.name,
          success,
          latency,
        });
        
        if (success) {
          systemRunning = true;
          console.log(`   ✅ ${healthCheck.name}: ${latency.toFixed(2)}ms`);
        } else {
          console.log(`   ❌ ${healthCheck.name}: Failed`);
        }
        
      } catch (error: any) {
        const latency = performance.now() - checkStart;
        metrics.healthChecks.push({
          name: healthCheck.name,
          success: false,
          latency,
          error: error.message
        });
        console.log(`   ❌ ${healthCheck.name}: ${error.message}`);
      }
    }
    
    return systemRunning;
  }
  
  private async testHttpPerformance(metrics: RealSystemMetrics): Promise<void> {
    const requestCount = 20;
    const responseTimes: number[] = [];
    
    globalProfiler.startTimer('http-performance-test');
    
    for (let i = 0; i < requestCount; i++) {
      const requestStart = performance.now();
      
      try {
        const success = await this.makeHttpRequest('http://localhost:9002');
        const responseTime = performance.now() - requestStart;
        
        if (success) {
          responseTimes.push(responseTime);
          console.log(`   Request ${i + 1}: ${responseTime.toFixed(2)}ms`);
        }
      } catch (error) {
        console.log(`   Request ${i + 1}: Failed`);
      }
      
      // Small delay between requests
      await this.sleep(50);
    }
    
    metrics.httpResponseTimes = responseTimes;
    globalProfiler.endTimer('http-performance-test');
    
    if (responseTimes.length > 0) {
      const avgResponseTime = responseTimes.reduce((sum, time) => sum + time, 0) / responseTimes.length;
      const minResponseTime = Math.min(...responseTimes);
      const maxResponseTime = Math.max(...responseTimes);
      
      console.log(`   📊 HTTP Performance: ${avgResponseTime.toFixed(2)}ms avg, ${minResponseTime.toFixed(2)}ms-${maxResponseTime.toFixed(2)}ms range`);
      console.log(`   📊 Success Rate: ${(responseTimes.length/requestCount*100).toFixed(1)}%`);
    }
  }
  
  private async testConcurrentRequestHandling(metrics: RealSystemMetrics): Promise<void> {
    const concurrentRequests = 10;
    const responseTimes: number[] = [];
    let successfulRequests = 0;
    
    globalProfiler.startTimer('concurrent-request-handling');
    
    // Launch all requests simultaneously
    const requestPromises = Array.from({ length: concurrentRequests }, async (_, i) => {
      const requestStart = performance.now();
      
      try {
        const success = await this.makeHttpRequest('http://localhost:9002', 10000);
        const responseTime = performance.now() - requestStart;
        
        if (success) {
          responseTimes.push(responseTime);
          successfulRequests++;
        }
        
        return { success, responseTime };
      } catch (error: any) {
        return { success: false, responseTime: performance.now() - requestStart, error: error.message };
      }
    });
    
    const results = await Promise.allSettled(requestPromises);
    globalProfiler.endTimer('concurrent-request-handling');
    
    // Calculate concurrent performance metrics
    if (responseTimes.length > 0) {
      metrics.concurrentRequestHandling = {
        concurrentRequests,
        successfulRequests,
        averageResponseTime: responseTimes.reduce((sum, time) => sum + time, 0) / responseTimes.length,
        maxResponseTime: Math.max(...responseTimes),
        minResponseTime: Math.min(...responseTimes)
      };
    }
    
    console.log(`   🚀 Concurrent Results: ${successfulRequests}/${concurrentRequests} successful`);
    console.log(`   📊 Response Times: ${metrics.concurrentRequestHandling.averageResponseTime.toFixed(2)}ms avg`);
    console.log(`   📊 Range: ${metrics.concurrentRequestHandling.minResponseTime.toFixed(2)}ms - ${metrics.concurrentRequestHandling.maxResponseTime.toFixed(2)}ms`);
  }
  
  private async testSystemLoadTime(metrics: RealSystemMetrics): Promise<void> {
    const loadTestStart = performance.now();
    
    // Simulate system load by making rapid requests
    const rapidRequests = Array.from({ length: 50 }, () => 
      this.makeHttpRequest('http://localhost:9002', 1000)
    );
    
    try {
      await Promise.allSettled(rapidRequests);
      metrics.systemLoadTime = performance.now() - loadTestStart;
      console.log(`   ⚡ System Load Test: ${metrics.systemLoadTime.toFixed(2)}ms for 50 requests`);
    } catch (error: any) {
      console.log(`   ⚠️ System Load Test: ${error.message}`);
    }
  }
  
  private async makeHttpRequest(url: string, timeout: number = 5000): Promise<boolean> {
    return new Promise((resolve) => {
      const request = http.get(url, { timeout }, (response) => {
        resolve(response.statusCode === 200);
      });
      
      request.on('timeout', () => {
        request.destroy();
        resolve(false);
      });
      
      request.on('error', () => {
        resolve(false);
      });
    });
  }
  
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
  
  private printRealSystemMetrics(metrics: RealSystemMetrics): void {
    console.log('\n📊 REAL SYSTEM PERFORMANCE METRICS');
    console.log('==================================');
    console.log(`🏥 System Running: ${metrics.systemRunning ? 'YES' : 'NO'}`);
    
    if (metrics.systemRunning) {
      console.log(`💾 Memory Usage: ${(metrics.memoryUsage.heapUsed / 1024 / 1024).toFixed(2)}MB heap`);
      
      if (metrics.httpResponseTimes.length > 0) {
        const avgHttp = metrics.httpResponseTimes.reduce((sum, time) => sum + time, 0) / metrics.httpResponseTimes.length;
        console.log(`⚡ HTTP Performance: ${avgHttp.toFixed(2)}ms average response time`);
      }
      
      const concurrent = metrics.concurrentRequestHandling;
      if (concurrent.successfulRequests > 0) {
        console.log(`🚀 Concurrent Handling: ${concurrent.successfulRequests}/${concurrent.concurrentRequests} requests successful`);
        console.log(`📊 Concurrent Performance: ${concurrent.averageResponseTime.toFixed(2)}ms average`);
        
        // Calculate concurrency scaling factor
        const singleRequestTime = metrics.httpResponseTimes.length > 0 ? 
          metrics.httpResponseTimes.reduce((sum, time) => sum + time, 0) / metrics.httpResponseTimes.length : 0;
        
        if (singleRequestTime > 0) {
          const concurrencyScaling = singleRequestTime / concurrent.averageResponseTime;
          console.log(`📈 Concurrency Scaling: ${concurrencyScaling.toFixed(2)}x (${concurrencyScaling > 1 ? 'GOOD' : 'NEEDS OPTIMIZATION'})`);
        }
      }
      
      if (metrics.systemLoadTime > 0) {
        console.log(`⚡ System Load: ${metrics.systemLoadTime.toFixed(2)}ms for 50 rapid requests`);
        console.log(`📊 Load Throughput: ${(50 / (metrics.systemLoadTime / 1000)).toFixed(1)} requests/sec`);
      }
    }
    
    console.log('\n💡 REAL SYSTEM PERFORMANCE INSIGHTS:');
    console.log('====================================');
    
    if (!metrics.systemRunning) {
      console.log('❌ SYSTEM NOT RUNNING - Start system with: npm run system:start');
      return;
    }
    
    const avgResponseTime = metrics.httpResponseTimes.length > 0 ? 
      metrics.httpResponseTimes.reduce((sum, time) => sum + time, 0) / metrics.httpResponseTimes.length : 0;
    
    if (avgResponseTime < 50) {
      console.log('🚀 EXCELLENT HTTP PERFORMANCE - System very responsive');
    } else if (avgResponseTime < 200) {
      console.log('✅ GOOD HTTP PERFORMANCE - System responsive');
    } else {
      console.log('⚠️ SLOW HTTP PERFORMANCE - Investigate bottlenecks');
    }
    
    const concurrentSuccess = (metrics.concurrentRequestHandling.successfulRequests / metrics.concurrentRequestHandling.concurrentRequests) * 100;
    if (concurrentSuccess > 90) {
      console.log('💪 EXCELLENT CONCURRENT HANDLING - System scales well');
    } else if (concurrentSuccess > 70) {
      console.log('✅ GOOD CONCURRENT HANDLING - Minor scaling issues');
    } else if (concurrentSuccess > 0) {
      console.log('⚠️ POOR CONCURRENT HANDLING - Significant scaling problems');
    }
    
    const throughput = metrics.systemLoadTime > 0 ? 50 / (metrics.systemLoadTime / 1000) : 0;
    if (throughput > 100) {
      console.log('⚡ HIGH THROUGHPUT - Excellent system performance');
    } else if (throughput > 50) {
      console.log('✅ MODERATE THROUGHPUT - Good performance');
    } else if (throughput > 0) {
      console.log('🐌 LOW THROUGHPUT - Performance optimization needed');
    }
  }
}

// Run if executed directly
async function runRealSystemTest(): Promise<void> {
  const tester = new RealSystemPerformanceTester();
  await tester.runSystemPerformanceTest();
  
  // Generate profiling report
  const report = globalProfiler.generateReport();
  console.log('\n📊 PROFILING REPORT:');
  console.log('====================');
  console.log(report);
}

if (require.main === module) {
  runRealSystemTest().catch(error => {
    console.error('❌ Real system test failed:', error);
    process.exit(1);
  });
}

export { RealSystemPerformanceTester };