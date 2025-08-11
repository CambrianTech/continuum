/**
 * Routing Chaos Browser Command - Complex Multi-Hop Routing Test (Browser Side)
 * 
 * Browser-side implementation of the routing chaos test. Works with the server
 * command to create complex routing patterns that test the router's resilience.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { 
  type RoutingChaosParams, 
  type RoutingChaosResult,
  ROUTING_CHAOS_COMMAND_PATH,
  generateTestPayload,
  shouldInjectError,
  generateRandomError
} from '../shared/RoutingChaosTypes';

export class RoutingChaosBrowserCommand extends CommandBase<RoutingChaosParams, RoutingChaosResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('routing-chaos', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RoutingChaosResult> {
    const chaosParams = params as RoutingChaosParams;
    const startTime = Date.now();
    const routingTrace: RoutingChaosResult['routingTrace'] = [];
    
    try {
      console.log(`🎲 BROWSER: Starting routing chaos test ${chaosParams.testId} (hop ${chaosParams.hopCount}/${chaosParams.maxHops})`);
      console.log(`🎲 BROWSER: Full params received:`, JSON.stringify(chaosParams, null, 2));
      
      // Record current hop
      const hopStart = Date.now();
      chaosParams.routingPath.push(`browser/hop-${chaosParams.hopCount}`);
      chaosParams.correlationTrace.push(`browser-${this.context.uuid}-${Date.now()}`);
      
      // Check if we've reached max hops
      if (chaosParams.hopCount >= chaosParams.maxHops) {
        console.log(`✅ BROWSER: Reached max hops for test ${chaosParams.testId}`);
        return this.createSuccessResult(chaosParams, routingTrace, startTime);
      }
      
      // Inject random failure if configured
      if (shouldInjectError(chaosParams.failureRate)) {
        const error = generateRandomError(['timeout', 'rejection', 'corruption']);
        console.log(`❌ BROWSER: Injecting random error for test ${chaosParams.testId}: ${error.message}`);
        
        routingTrace.push({
          hop: chaosParams.hopCount,
          from: 'browser',
          to: 'error',
          durationMs: Date.now() - hopStart,
          success: false,
          error: error.message,
          timestamp: new Date().toISOString()
        });
        
        throw error;
      }
      
      // Add random delay (browser-specific delays)
      const delay = Math.floor(Math.random() * (chaosParams.delayRange[1] - chaosParams.delayRange[0])) + chaosParams.delayRange[0];
      await new Promise(resolve => setTimeout(resolve, delay));
      
      // Browser-specific operations (simulate DOM interactions, storage, etc.)
      await this.simulateBrowserOperations(chaosParams);
      
      // Decide next hop (browsers typically route back to server or other browser contexts)
      const nextEnvironment = Math.random() > 0.3 ? 'server' : 'browser'; // Favor server routing
      const nextHopCount = chaosParams.hopCount + 1;
      
      console.log(`🔄 BROWSER: Routing to ${nextEnvironment} for hop ${nextHopCount} of test ${chaosParams.testId}`);
      
      const hopEnd = Date.now();
      routingTrace.push({
        hop: chaosParams.hopCount,
        from: 'browser',
        to: nextEnvironment,
        durationMs: hopEnd - hopStart,
        success: true,
        timestamp: new Date().toISOString()
      });
      
      // Create next hop message
      const nextParams: RoutingChaosParams = {
        ...chaosParams,
        hopCount: nextHopCount,
        currentEnvironment: 'browser',
        targetEnvironment: nextEnvironment as 'browser' | 'server'
      };
      
      // Route to next environment using remoteExecute
      if (nextEnvironment === 'server') {
        // Route to server
        const serverResult = await this.remoteExecute(nextParams, ROUTING_CHAOS_COMMAND_PATH, 'server');
        return this.mergeResults(serverResult, routingTrace);
      } else {
        // Route to another browser context (simulate multi-tab or service worker scenario)
        const browserResult = await this.remoteExecute(nextParams, ROUTING_CHAOS_COMMAND_PATH, 'browser');
        return this.mergeResults(browserResult, routingTrace);
      }
      
    } catch (error: any) {
      console.error(`❌ BROWSER: Routing chaos test ${chaosParams.testId} failed at hop ${chaosParams.hopCount}:`, error.message);
      
      return {
        context: this.context,
        sessionId: this.context.uuid,
        testId: chaosParams.testId,
        success: false,
        totalHops: chaosParams.hopCount,
        actualPath: chaosParams.routingPath,
        totalDurationMs: Date.now() - startTime,
        errorEncountered: error.message,
        performanceMetrics: {
          hopDurations: [Date.now() - startTime],
          averageHopTime: Date.now() - startTime,
          slowestHop: Date.now() - startTime,
          fastestHop: Date.now() - startTime,
          totalCorrelations: chaosParams.correlationTrace.length,
          failedHops: 1
        },
        routingTrace
      };
    }
  }
  
  private async simulateBrowserOperations(params: RoutingChaosParams): Promise<void> {
    // Simulate browser-specific operations that might affect routing
    
    // Simulate localStorage operations
    try {
      const testKey = `routing-chaos-${params.testId}`;
      const testData = generateTestPayload(params.payloadSize);
      
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(testKey, JSON.stringify(testData));
        const retrieved = localStorage.getItem(testKey);
        if (!retrieved) {
          throw new Error('localStorage operation failed');
        }
        // Clean up
        localStorage.removeItem(testKey);
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: localStorage operation failed in test ${params.testId}:`, error);
    }
    
    // Simulate DOM manipulation (if in browser environment)
    try {
      if (typeof document !== 'undefined') {
        const testElement = document.createElement('div');
        testElement.id = `routing-chaos-${params.testId}`;
        testElement.textContent = `Chaos test hop ${params.hopCount}`;
        document.body.appendChild(testElement);
        
        // Brief delay to simulate DOM operations
        await new Promise(resolve => setTimeout(resolve, 1));
        
        // Clean up
        testElement.remove();
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: DOM operation failed in test ${params.testId}:`, error);
    }
    
    // Simulate WebAPI calls that might affect performance
    try {
      if (typeof performance !== 'undefined') {
        const mark = `routing-chaos-${params.testId}-hop-${params.hopCount}`;
        performance.mark(mark);
        
        // Memory usage check if available
        if ('memory' in performance) {
          const memInfo = (performance as any).memory;
          console.log(`🧠 BROWSER: Memory usage at hop ${params.hopCount}: ${memInfo.usedJSHeapSize} bytes`);
        }
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: Performance API operation failed in test ${params.testId}:`, error);
    }
  }
  
  private createSuccessResult(
    params: RoutingChaosParams, 
    routingTrace: RoutingChaosResult['routingTrace'],
    startTime: number
  ): RoutingChaosResult {
    const totalDuration = Date.now() - startTime;
    const hopDurations = routingTrace.map(trace => trace.durationMs);
    
    return {
      context: this.context,
      sessionId: this.context.uuid,
      testId: params.testId,
      success: true,
      totalHops: params.hopCount,
      actualPath: params.routingPath,
      totalDurationMs: totalDuration,
      performanceMetrics: {
        hopDurations,
        averageHopTime: hopDurations.length > 0 ? hopDurations.reduce((a, b) => a + b, 0) / hopDurations.length : 0,
        slowestHop: hopDurations.length > 0 ? Math.max(...hopDurations) : 0,
        fastestHop: hopDurations.length > 0 ? Math.min(...hopDurations) : 0,
        totalCorrelations: params.correlationTrace.length,
        failedHops: 0
      },
      routingTrace
    };
  }
  
  private mergeResults(
    childResult: any, // remoteExecute can return wrapped responses
    currentTrace: RoutingChaosResult['routingTrace']
  ): RoutingChaosResult {
    console.log(`🔄 BROWSER: Merging results - childResult type:`, typeof childResult, 'keys:', Object.keys(childResult));
    console.log(`🔄 BROWSER: childResult.routingTrace:`, childResult.routingTrace, 'is array:', Array.isArray(childResult.routingTrace));
    
    // Handle wrapped response from remoteExecute
    let actualResult = childResult;
    if (childResult.handlerResult && childResult.handlerResult.commandResult) {
      console.log(`🔄 BROWSER: Unwrapping nested command result`);
      actualResult = childResult.handlerResult.commandResult;
    }
    
    // Ensure routingTrace is an array
    const childTrace = Array.isArray(actualResult.routingTrace) ? actualResult.routingTrace : [];
    console.log(`🔄 BROWSER: Using childTrace length:`, childTrace.length);
    
    return {
      ...actualResult,
      routingTrace: [...currentTrace, ...childTrace],
      performanceMetrics: {
        ...actualResult.performanceMetrics,
        hopDurations: [
          ...currentTrace.map(t => t.durationMs), 
          ...(actualResult.performanceMetrics?.hopDurations || [])
        ]
      }
    };
  }
}