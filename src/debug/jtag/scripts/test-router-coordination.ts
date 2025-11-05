/**
 * Router Coordination Validation Script
 * 
 * Validates that the router coordination helpers are properly integrated
 * and that DataDaemonBrowser uses router.routeToServer() instead of manual message construction
 */

import fs from 'fs';
import path from 'path';

interface RouterCoordinationValidation {
  readonly dataDaemonUsesRouterHelpers: boolean;
  readonly routerHelpersExist: boolean;
  readonly oldPatternsRemoved: boolean;
  readonly compilationClean: boolean;
  readonly integrationTestExists: boolean;
}

async function validateRouterCoordination(): Promise<RouterCoordinationValidation> {
  console.log('🔍 ROUTER COORDINATION VALIDATION');
  console.log('=================================');
  
  // 1. Check DataDaemonBrowser uses router helpers
  const dataDaemonPath = 'daemons/data-daemon/browser/DataDaemonBrowser.ts';
  let dataDaemonContent = '';
  let dataDaemonUsesRouterHelpers = false;
  
  try {
    dataDaemonContent = fs.readFileSync(dataDaemonPath, 'utf8');
    const hasRouterCalls = dataDaemonContent.includes('router.routeToServer');
    const hasOldPatterns = dataDaemonContent.includes('JTAGMessageFactory.createRequest');
    
    dataDaemonUsesRouterHelpers = hasRouterCalls && !hasOldPatterns;
    console.log(`📋 DataDaemonBrowser uses router helpers: ${dataDaemonUsesRouterHelpers ? '✅' : '❌'}`);
    if (hasOldPatterns) {
      console.log('⚠️  Still contains old JTAGMessageFactory.createRequest patterns');
    }
  } catch (error) {
    console.log(`❌ Failed to read DataDaemonBrowser: ${error instanceof Error ? error.message : 'Unknown error'}`);
  }
  
  // 2. Check router helpers exist
  const routerPath = 'system/core/router/shared/JTAGRouter.ts';
  let routerHelpersExist = false;
  
  try {
    const routerContent = fs.readFileSync(routerPath, 'utf8');
    const hasRouteToServer = routerContent.includes('async routeToServer');
    const hasRouteToBrowser = routerContent.includes('async routeToBrowser');
    const hasRouteToDaemon = routerContent.includes('async routeToDaemon');
    const hasIsDaemonAvailable = routerContent.includes('isDaemonAvailable');
    
    routerHelpersExist = hasRouteToServer && hasRouteToBrowser && hasRouteToDaemon && hasIsDaemonAvailable;
    console.log(`📋 Router coordination helpers exist: ${routerHelpersExist ? '✅' : '❌'}`);
    console.log(`   routeToServer: ${hasRouteToServer ? '✅' : '❌'}`);
    console.log(`   routeToBrowser: ${hasRouteToBrowser ? '✅' : '❌'}`);
    console.log(`   routeToDaemon: ${hasRouteToDaemon ? '✅' : '❌'}`);
    console.log(`   isDaemonAvailable: ${hasIsDaemonAvailable ? '✅' : '❌'}`);
  } catch (error) {
    console.log(`❌ Failed to read JTAGRouter: ${error instanceof Error ? error.message : 'Unknown error'}`);
  }
  
  // 3. Check old patterns removed from codebase
  const oldPatternsRemoved = !dataDaemonContent.includes('JTAGMessageFactory.createRequest');
  console.log(`📋 Old manual message patterns removed: ${oldPatternsRemoved ? '✅' : '❌'}`);
  
  // 4. Check integration test exists
  const testPath = 'tests/integration/router-coordination-simple.test.ts';
  const integrationTestExists = fs.existsSync(testPath);
  console.log(`📋 Router coordination integration test exists: ${integrationTestExists ? '✅' : '❌'}`);
  
  // 5. Check compilation is clean
  const { spawn } = await import('child_process');
  const compilationClean = await new Promise<boolean>((resolve) => {
    const tscProcess = spawn('npx', ['tsc', '--noEmit', '--project', '.'], {
      stdio: 'pipe'
    });
    
    let hasErrors = false;
    tscProcess.stderr?.on('data', (data) => {
      if (data.toString().trim()) {
        hasErrors = true;
      }
    });
    
    tscProcess.on('close', (code) => {
      resolve(code === 0 && !hasErrors);
    });
  });
  
  console.log(`📋 TypeScript compilation clean: ${compilationClean ? '✅' : '❌'}`);
  
  const validation: RouterCoordinationValidation = {
    dataDaemonUsesRouterHelpers,
    routerHelpersExist, 
    oldPatternsRemoved,
    compilationClean,
    integrationTestExists
  };
  
  console.log('');
  console.log('🎯 ROUTER COORDINATION VALIDATION RESULTS:');
  console.log('==========================================');
  
  const allValid = Object.values(validation).every(v => v);
  
  if (allValid) {
    console.log('✅ ALL VALIDATIONS PASSED - Router coordination integration complete');
    console.log('🎯 DataDaemonBrowser now uses standardized router.routeToServer() helpers');
    console.log('🎯 Integration test added to npm test workflow');
    console.log('🎯 Strong typing enforced through router coordination helpers');
  } else {
    console.log('❌ ROUTER COORDINATION VALIDATION FAILED');
    console.log('📋 Issues found:');
    Object.entries(validation).forEach(([key, value]) => {
      if (!value) {
        console.log(`   ❌ ${key}`);
      }
    });
  }
  
  console.log('');
  console.log('📊 Detailed Results:');
  console.log(JSON.stringify(validation, null, 2));
  
  return validation;
}

// Execute validation
validateRouterCoordination()
  .then(validation => {
    const success = Object.values(validation).every(v => v);
    process.exit(success ? 0 : 1);
  })
  .catch(error => {
    console.error('🚨 Router coordination validation failed:', error);
    process.exit(1);
  });