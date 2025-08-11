/**
 * Routing Chaos Server Command - Complex Multi-Hop Routing Test
 * 
 * A diagnostic command that tests the router's ability to handle complex routing scenarios:
 * - Multi-hop routing chains (browser->server->server->browser->server)
 * - Random success/failure scenarios with proper error propagation
 * - Promise resolution across chaotic routing paths
 * - Performance metrics under stress
 * 
 * This command can be left in as a permanent diagnostic tool for production systems.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { 
  type RoutingChaosParams, 
  type RoutingChaosResult,
  type RoutingChainTestParams,
  type RoutingChainTestResult,
  ROUTING_CHAOS_COMMAND_PATH,
  generateTestPayload,
  generateRandomRoutingPath,
  shouldInjectError,
  generateRandomError
} from '../shared/RoutingChaosTypes';

export class RoutingChaosServerCommand extends CommandBase<RoutingChaosParams, RoutingChaosResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('routing-chaos', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RoutingChaosResult> {
    const chaosParams = params as RoutingChaosParams;
    const startTime = Date.now();
    const routingTrace: RoutingChaosResult['routingTrace'] = [];
    
    try {
      console.log(`🎲 SERVER: Starting routing chaos test ${chaosParams.testId} (hop ${chaosParams.hopCount}/${chaosParams.maxHops})`);
      console.log(`🎲 SERVER: Full params received:`, JSON.stringify(chaosParams, null, 2));
      
      // Record current hop
      const hopStart = Date.now();
      chaosParams.routingPath.push(`server/hop-${chaosParams.hopCount}`);
      chaosParams.correlationTrace.push(`server-${this.context.uuid}-${Date.now()}`);
      
      // Check if we've reached max hops
      if (chaosParams.hopCount >= chaosParams.maxHops) {
        console.log(`✅ SERVER: Reached max hops for test ${chaosParams.testId}`);
        return this.createSuccessResult(chaosParams, routingTrace, startTime);
      }
      
      // Inject random failure if configured
      if (shouldInjectError(chaosParams.failureRate)) {
        const error = generateRandomError(['timeout', 'rejection', 'corruption']);
        console.log(`❌ SERVER: Injecting random error for test ${chaosParams.testId}: ${error.message}`);
        
        routingTrace.push({
          hop: chaosParams.hopCount,
          from: 'server',
          to: 'error',
          durationMs: Date.now() - hopStart,
          success: false,
          error: error.message,
          timestamp: new Date().toISOString()
        });
        
        throw error;
      }
      
      // Add random delay
      const delay = Math.floor(Math.random() * (chaosParams.delayRange[1] - chaosParams.delayRange[0])) + chaosParams.delayRange[0];
      await new Promise(resolve => setTimeout(resolve, delay));
      
      // Decide next hop randomly
      const nextEnvironment = Math.random() > 0.5 ? 'browser' : 'server';
      const nextHopCount = chaosParams.hopCount + 1;
      
      console.log(`🔄 SERVER: Routing to ${nextEnvironment} for hop ${nextHopCount} of test ${chaosParams.testId}`);
      
      const hopEnd = Date.now();
      routingTrace.push({
        hop: chaosParams.hopCount,
        from: 'server',
        to: nextEnvironment,
        durationMs: hopEnd - hopStart,
        success: true,
        timestamp: new Date().toISOString()
      });
      
      // Create next hop message
      const nextParams: RoutingChaosParams = {
        ...chaosParams,
        hopCount: nextHopCount,
        currentEnvironment: 'server',
        targetEnvironment: nextEnvironment as 'browser' | 'server'
      };
      
      // Route to next environment using remoteExecute
      if (nextEnvironment === 'browser') {
        console.log(`🔄 SERVER: About to execute remoteExecute to browser with params:`, JSON.stringify(nextParams, null, 2));
        // Route to browser
        const browserResult = await this.remoteExecute(nextParams, ROUTING_CHAOS_COMMAND_PATH, 'browser');
        console.log(`✅ SERVER: Received result from browser:`, JSON.stringify(browserResult, null, 2));
        return this.mergeResults(browserResult, routingTrace);
      } else {
        console.log(`🔄 SERVER: About to execute remoteExecute to server with params:`, JSON.stringify(nextParams, null, 2));
        // Route to another server instance (simulate distributed server environment)
        const serverResult = await this.remoteExecute(nextParams, ROUTING_CHAOS_COMMAND_PATH, 'server');
        console.log(`✅ SERVER: Received result from server:`, JSON.stringify(serverResult, null, 2));
        return this.mergeResults(serverResult, routingTrace);
      }
      
    } catch (error: any) {
      console.error(`❌ SERVER: Routing chaos test ${chaosParams.testId} failed at hop ${chaosParams.hopCount}:`, error.message);
      
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
    console.log(`🔄 SERVER: Merging results - childResult type:`, typeof childResult, 'keys:', Object.keys(childResult));
    console.log(`🔄 SERVER: childResult.routingTrace:`, childResult.routingTrace, 'is array:', Array.isArray(childResult.routingTrace));
    
    // Handle wrapped response from remoteExecute
    let actualResult = childResult;
    if (childResult.handlerResult && childResult.handlerResult.commandResult) {
      console.log(`🔄 SERVER: Unwrapping nested command result`);
      actualResult = childResult.handlerResult.commandResult;
    }
    
    // Ensure routingTrace is an array
    const childTrace = Array.isArray(actualResult.routingTrace) ? actualResult.routingTrace : [];
    console.log(`🔄 SERVER: Using childTrace length:`, childTrace.length);
    
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

/**
 * Routing Chain Test Command - Tests complex parallel routing chains
 */
export class RoutingChainTestServerCommand extends CommandBase<RoutingChainTestParams, RoutingChainTestResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('routing-chain-test', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<RoutingChainTestResult> {
    const testParams = params as RoutingChainTestParams;
    const testStartTime = Date.now();
    console.log(`🔗 SERVER: Starting routing chain test ${testParams.chainId} with ${testParams.branchFactor} parallel chains`);
    
    try {
      // Create multiple parallel routing chains
      const chainPromises: Array<Promise<any>> = [];
      
      for (let i = 0; i < testParams.branchFactor; i++) {
        const chainPromise = this.executeRoutingChain(i, testParams);
        chainPromises.push(chainPromise);
      }
      
      // Execute all chains and collect results
      const chainResults = await Promise.allSettled(chainPromises);
      
      // Analyze results
      const successfulChains = chainResults.filter(r => r.status === 'fulfilled').length;
      const failedChains = chainResults.length - successfulChains;
      const totalDuration = Date.now() - testStartTime;
      
      const errorBreakdown: Record<string, number> = {};
      const chainResultDetails: RoutingChainTestResult['chainResults'] = [];
      
      chainResults.forEach((result, index) => {
        if (result.status === 'fulfilled') {
          chainResultDetails.push({
            chainIndex: index,
            success: true,
            durationMs: result.value.durationMs || 0,
            hopCount: result.value.hopCount || 0,
            finalResult: result.value
          });
        } else {
          const errorType = result.reason?.name || 'UnknownError';
          errorBreakdown[errorType] = (errorBreakdown[errorType] || 0) + 1;
          
          chainResultDetails.push({
            chainIndex: index,
            success: false,
            durationMs: 0,
            hopCount: 0,
            errorType
          });
        }
      });
      
      const averageChainDuration = chainResultDetails
        .filter(r => r.success)
        .reduce((sum, r) => sum + r.durationMs, 0) / (successfulChains || 1);
      
      console.log(`✅ SERVER: Chain test ${testParams.chainId} completed - ${successfulChains}/${testParams.branchFactor} successful`);
      
      return {
        context: this.context,
        sessionId: this.context.uuid,
        chainId: testParams.chainId,
        totalChains: testParams.branchFactor,
        successfulChains,
        failedChains,
        averageChainDurationMs: averageChainDuration,
        totalTestDurationMs: totalDuration,
        errorBreakdown,
        performanceMetrics: {
          averageLatencyMs: averageChainDuration,
          throughputChainsPerSecond: testParams.branchFactor / (totalDuration / 1000),
          correlationEfficiency: successfulChains / testParams.branchFactor
        },
        chainResults: chainResultDetails
      };
      
    } catch (error: any) {
      console.error(`❌ SERVER: Chain test ${testParams.chainId} failed:`, error.message);
      throw error;
    }
  }
  
  private async executeRoutingChain(chainIndex: number, params: RoutingChainTestParams): Promise<any> {
    const chainStartTime = Date.now();
    
    // Create a routing chaos test for this chain
    const chaosTestParams = {
      context: this.context,
      sessionId: this.context.uuid,
      testId: `${params.chainId}-chain-${chainIndex}`,
      hopCount: 0,
      maxHops: params.chainDepth,
      routingPath: [],
      currentEnvironment: 'server' as const,
      failureRate: params.errorInjection.enabled ? params.errorInjection.errorRate : 0,
      delayRange: [5, 50] as [number, number],
      payloadSize: 'small' as const,
      testStartTime: new Date().toISOString(),
      correlationTrace: []
    };
    
    // Execute the chaos test
    const result = await this.execute(chaosTestParams as any);
    
    return {
      ...result,
      durationMs: Date.now() - chainStartTime,
      hopCount: result.totalChains
    };
  }
}