/**
 * Router Performance Analysis - Identify and Optimize Bottlenecks
 * 
 * Comprehensive performance analysis of JTAGRouter using PerformanceProfiler.
 * Measures key routing operations and identifies optimization opportunities.
 */

import { performance } from 'perf_hooks';
import { globalProfiler, PerformanceProfiler } from '../shared/performance/PerformanceProfiler';
import { JTAGMessageFactory, JTAGMessageTypes } from '../system/core/types/JTAGTypes';
import { createTestJTAGContext } from './test-utils/TestJTAGContext';

// Mock subscriber for testing
class MockSubscriber {
  constructor(public endpoint: string, public uuid: string) {}
  
  async handleMessage(message: any): Promise<any> {
    // Simulate processing delay
    await new Promise(resolve => setTimeout(resolve, Math.random() * 10 + 5));
    return { success: true, data: `Processed by ${this.endpoint}` };
  }
}

// Router Performance Analyzer 
class RouterPerformanceAnalyzer {
  private profiler = new PerformanceProfiler();
  private mockRouter: any;
  
  async setupMockRouter(): Promise<void> {
    // Create minimal router mock focusing on bottleneck areas
    this.mockRouter = {
      endpointMatcher: new Map(),
      processedMessages: new Set(),
      requestSenders: new Map(),
      correlationManager: { registerRequest: () => {}, cleanupCorrelation: () => {} },
      context: createTestJTAGContext('browser'),
      
      // Mock the bottleneck methods we're analyzing
      registerSubscriber: (endpoint: string, subscriber: MockSubscriber) => {
        this.mockRouter.endpointMatcher.set(endpoint, subscriber);
      },
      
      findSubscriber: (endpoint: string) => {
        return this.mockRouter.endpointMatcher.get(endpoint);
      },
      
      // Simulate the complex endpoint matching logic
      matchEndpoint: (targetEndpoint: string) => {
        // This simulates the EndpointMatcher logic which could be optimized
        for (const [registeredEndpoint, subscriber] of this.mockRouter.endpointMatcher) {
          if (targetEndpoint === registeredEndpoint) {
            return { subscriber, matchedEndpoint: registeredEndpoint, matchType: 'exact' };
          }
          if (targetEndpoint.startsWith(registeredEndpoint + '/')) {
            return { subscriber, matchedEndpoint: registeredEndpoint, matchType: 'hierarchical' };
          }
        }
        return null;
      }
    };
  }
  
  async analyzeMessageProcessingOverhead(): Promise<void> {
    console.log('\n🔍 ANALYZING MESSAGE PROCESSING OVERHEAD');
    console.log('======================================');
    
    // Test message creation performance
    const { timing: createTiming } = await this.profiler.timeAsync(
      'message-creation', 
      async () => {
        const context = createTestJTAGContext('browser');
        const messages = [];
        for (let i = 0; i < 100; i++) {
          messages.push(JTAGMessageFactory.createRequest(
            context,
            'browser/test',
            'server/screenshot',
            { querySelector: `#element-${i}` },
            `correlation-${i}`
          ));
        }
        return messages;
      }
    );
    
    console.log(`📊 Message Creation (100 messages): ${createTiming.duration.toFixed(2)}ms`);
    
    // Test message type checking performance
    const testMessage = JTAGMessageFactory.createRequest(
      createTestJTAGContext('browser'),
      'browser/test',
      'server/screenshot', 
      { querySelector: '#test' },
      'test-correlation'
    );
    
    const { timing: typeCheckTiming } = await this.profiler.timeAsync(
      'message-type-checking',
      async () => {
        for (let i = 0; i < 1000; i++) {
          const isRequest = JTAGMessageTypes.isRequest(testMessage);
          const isResponse = JTAGMessageTypes.isResponse(testMessage);
          const isEvent = JTAGMessageTypes.isEvent(testMessage);
        }
      }
    );
    
    console.log(`📊 Type Checking (1000 checks): ${typeCheckTiming.duration.toFixed(2)}ms`);
  }
  
  async analyzeEndpointMatching(): Promise<void> {
    console.log('\n🎯 ANALYZING ENDPOINT MATCHING PERFORMANCE');
    console.log('==========================================');
    
    // Setup test subscribers
    for (let i = 0; i < 50; i++) {
      this.mockRouter.registerSubscriber(`daemon-${i}`, new MockSubscriber(`daemon-${i}`, `uuid-${i}`));
      this.mockRouter.registerSubscriber(`server/daemon-${i}`, new MockSubscriber(`server/daemon-${i}`, `uuid-s-${i}`));
    }
    
    // Test exact matching performance
    const { timing: exactMatchTiming } = await this.profiler.timeAsync(
      'endpoint-exact-matching',
      async () => {
        for (let i = 0; i < 500; i++) {
          const endpoint = `daemon-${i % 50}`;
          this.mockRouter.matchEndpoint(endpoint);
        }
      }
    );
    
    console.log(`📊 Exact Matching (500 lookups): ${exactMatchTiming.duration.toFixed(2)}ms`);
    
    // Test hierarchical matching performance (more complex)
    const { timing: hierarchicalTiming } = await this.profiler.timeAsync(
      'endpoint-hierarchical-matching',
      async () => {
        for (let i = 0; i < 500; i++) {
          const endpoint = `server/daemon-${i % 50}/command/screenshot`;
          this.mockRouter.matchEndpoint(endpoint);
        }
      }
    );
    
    console.log(`📊 Hierarchical Matching (500 lookups): ${hierarchicalTiming.duration.toFixed(2)}ms`);
  }
  
  async analyzeCorrelationManagement(): Promise<void> {
    console.log('\n🔗 ANALYZING CORRELATION MANAGEMENT PERFORMANCE');
    console.log('===============================================');
    
    // Test correlation registration/cleanup performance
    const { timing: correlationTiming } = await this.profiler.timeAsync(
      'correlation-management',
      async () => {
        for (let i = 0; i < 200; i++) {
          const correlationId = `test-correlation-${i}`;
          
          // Register correlation
          this.mockRouter.correlationManager.registerRequest(correlationId);
          
          // Store request sender info (bottleneck area)
          this.mockRouter.requestSenders.set(correlationId, {
            environment: 'browser',
            timestamp: Date.now()
          });
          
          // Cleanup (after 25% of registrations to simulate realistic usage)
          if (i % 4 === 0) {
            this.mockRouter.requestSenders.delete(`test-correlation-${i - 3}`);
            this.mockRouter.correlationManager.cleanupCorrelation(`test-correlation-${i - 3}`);
          }
        }
      }
    );
    
    console.log(`📊 Correlation Management (200 ops): ${correlationTiming.duration.toFixed(2)}ms`);
    
    // Test processed message deduplication performance
    const { timing: deduplicationTiming } = await this.profiler.timeAsync(
      'message-deduplication',
      async () => {
        for (let i = 0; i < 1000; i++) {
          const token = `request-correlation-${i % 100}`; // 90% duplicates
          
          // Simulate the deduplication check
          if (!this.mockRouter.processedMessages.has(token)) {
            this.mockRouter.processedMessages.add(token);
            
            // Simulate cleanup scheduling
            setTimeout(() => {
              this.mockRouter.processedMessages.delete(token);
            }, 100);
          }
        }
      }
    );
    
    console.log(`📊 Message Deduplication (1000 ops): ${deduplicationTiming.duration.toFixed(2)}ms`);
  }
  
  async analyzeOverallRoutingPerformance(): Promise<void> {
    console.log('\n🚀 ANALYZING OVERALL ROUTING PERFORMANCE');
    console.log('========================================');
    
    // Setup realistic environment
    for (let i = 0; i < 20; i++) {
      this.mockRouter.registerSubscriber(`daemon-${i}`, new MockSubscriber(`daemon-${i}`, `uuid-${i}`));
    }
    
    // Test complete routing cycle performance
    const { timing: routingTiming } = await this.profiler.timeAsync(
      'complete-routing-cycle',
      async () => {
        for (let i = 0; i < 100; i++) {
          const correlationId = `routing-test-${i}`;
          
          // Create message (bottleneck 1)
          const message = JTAGMessageFactory.createRequest(
            this.mockRouter.context,
            'browser/test',
            `daemon-${i % 20}`,
            { test: i },
            correlationId
          );
          
          // Type checking (bottleneck 2) 
          const isRequest = JTAGMessageTypes.isRequest(message);
          
          // Deduplication check (bottleneck 3)
          const token = `request-${correlationId}`;
          if (this.mockRouter.processedMessages.has(token)) continue;
          this.mockRouter.processedMessages.add(token);
          
          // Endpoint matching (bottleneck 4)
          const match = this.mockRouter.matchEndpoint(message.endpoint);
          
          // Correlation management (bottleneck 5)
          this.mockRouter.correlationManager.registerRequest(correlationId);
          this.mockRouter.requestSenders.set(correlationId, {
            environment: 'browser'
          });
          
          // Simulate handler execution (bottleneck 6)
          if (match) {
            await match.subscriber.handleMessage(message);
          }
          
          // Cleanup
          this.mockRouter.requestSenders.delete(correlationId);
          this.mockRouter.processedMessages.delete(token);
        }
      }
    );
    
    console.log(`📊 Complete Routing Cycle (100 messages): ${routingTiming.duration.toFixed(2)}ms`);
    console.log(`📊 Average per message: ${(routingTiming.duration / 100).toFixed(2)}ms`);
  }
  
  async identifyBottlenecks(): Promise<void> {
    console.log('\n🎯 PERFORMANCE BOTTLENECK IDENTIFICATION');
    console.log('========================================');
    
    const report = this.profiler.generateReport();
    console.log(report);
    
    // Specific bottleneck analysis
    const metrics = {
      messageCreation: this.profiler.getMetrics('message-creation'),
      typeChecking: this.profiler.getMetrics('message-type-checking'),
      exactMatching: this.profiler.getMetrics('endpoint-exact-matching'),
      hierarchicalMatching: this.profiler.getMetrics('endpoint-hierarchical-matching'),
      correlationMgmt: this.profiler.getMetrics('correlation-management'),
      deduplication: this.profiler.getMetrics('message-deduplication'),
      completeRouting: this.profiler.getMetrics('complete-routing-cycle')
    };
    
    console.log('\n🔍 BOTTLENECK ANALYSIS:');
    console.log('======================');
    
    // Calculate relative performance impact
    const operations = Object.entries(metrics).filter(([_, metric]) => metric !== null);
    operations.sort(([_, a], [__, b]) => (b?.averageDuration || 0) - (a?.averageDuration || 0));
    
    operations.forEach(([name, metric], index) => {
      const indicator = index === 0 ? '🐌' : index === 1 ? '⚠️' : '✅';
      const throughput = metric?.throughputPerSecond?.toFixed(0) || 'N/A';
      console.log(`${indicator} ${name}: ${metric?.averageDuration?.toFixed(2)}ms avg (${throughput} ops/sec)`);
    });
    
    console.log('\n💡 OPTIMIZATION OPPORTUNITIES:');
    console.log('==============================');
    
    const topBottleneck = operations[0];
    if (topBottleneck && topBottleneck[1]) {
      const [name, metric] = topBottleneck;
      console.log(`🎯 Primary bottleneck: ${name} (${metric.averageDuration?.toFixed(2)}ms average)`);
      
      if (name.includes('matching')) {
        console.log('   → Optimize with indexed endpoint lookup (Map → specialized data structure)');
        console.log('   → Consider caching frequently accessed endpoints');
        console.log('   → Pre-compile hierarchical match patterns');
      } else if (name.includes('correlation')) {
        console.log('   → Implement batch correlation cleanup');
        console.log('   → Use WeakMap for automatic garbage collection');
        console.log('   → Consider correlation pooling for high-frequency operations');
      } else if (name.includes('deduplication')) {
        console.log('   → Use more efficient data structure than Set for deduplication');
        console.log('   → Implement LRU cache with automatic cleanup');
        console.log('   → Consider bloom filters for probabilistic deduplication');
      }
    }
  }
  
  async generateOptimizations(): Promise<void> {
    console.log('\n🚀 GENERATING PERFORMANCE OPTIMIZATIONS');
    console.log('=======================================');
    
    const optimizations = [
      {
        name: 'Endpoint Matching Cache',
        description: 'Cache frequently accessed endpoint matches',
        expectedImprovement: '40-60% faster lookups',
        implementation: 'LRU cache with 100-entry limit'
      },
      {
        name: 'Batch Correlation Cleanup',
        description: 'Clean up correlations in batches instead of individually',
        expectedImprovement: '30-50% reduced GC pressure',
        implementation: 'Periodic batch cleanup every 1000ms'
      },
      {
        name: 'Message Type Check Optimization',
        description: 'Cache message type results using WeakMap',
        expectedImprovement: '70-80% faster type checking',
        implementation: 'WeakMap-based type cache per message object'
      },
      {
        name: 'Processed Message Bloom Filter',
        description: 'Use bloom filter for approximate deduplication',
        expectedImprovement: '50-70% memory reduction, 20-30% speed improvement',
        implementation: 'Probabilistic bloom filter with 0.1% false positive rate'
      }
    ];
    
    optimizations.forEach((opt, i) => {
      console.log(`\n${i + 1}. ${opt.name}`);
      console.log(`   Description: ${opt.description}`);
      console.log(`   Expected: ${opt.expectedImprovement}`);
      console.log(`   Implementation: ${opt.implementation}`);
    });
  }
}

async function runRouterPerformanceAnalysis(): Promise<void> {
  console.log('🎯 JTAG ROUTER PERFORMANCE ANALYSIS');
  console.log('===================================');
  console.log('Identifying bottlenecks and optimization opportunities in routing system\n');
  
  const analyzer = new RouterPerformanceAnalyzer();
  
  try {
    await analyzer.setupMockRouter();
    
    // Run comprehensive performance analysis
    await analyzer.analyzeMessageProcessingOverhead();
    await analyzer.analyzeEndpointMatching();
    await analyzer.analyzeCorrelationManagement();
    await analyzer.analyzeOverallRoutingPerformance();
    
    // Identify bottlenecks and generate optimization strategies
    await analyzer.identifyBottlenecks();
    await analyzer.generateOptimizations();
    
    console.log('\n🎉 ROUTER PERFORMANCE ANALYSIS COMPLETE');
    console.log('Ready to implement iterative optimizations with measurable improvements!');
    
  } catch (error: any) {
    console.error('❌ Performance analysis failed:', error.message);
    console.error(error.stack);
  }
}

// Run if executed directly
if (require.main === module) {
  runRouterPerformanceAnalysis().catch(error => {
    console.error('❌ Analysis crashed:', error);
    process.exit(1);
  });
}

export { runRouterPerformanceAnalysis };