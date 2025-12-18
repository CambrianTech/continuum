#!/usr/bin/env tsx
/**
 * Improved System Detector v2 - Using modular health check framework
 * 
 * Cleaner, more maintainable version using the new HealthCheckFramework
 */

import { checkJTAGHealth, checkJTAGCritical } from '../shared/health/JTAGHealthSuite';

interface DetectorOptions {
  critical?: boolean;
  verbose?: boolean;
}

async function improvedSystemDetectorV2(options: DetectorOptions = {}): Promise<void> {
  console.log('🔍 IMPROVED SYSTEM DETECTOR V2');
  console.log('==============================');
  console.log('Using modular health check framework');
  console.log();

  try {
    const isHealthy = options.critical 
      ? await checkJTAGCritical()
      : await checkJTAGHealth();

    if (isHealthy) {
      console.log();
      console.log('🔧 SYSTEM READY - You can now:');
      console.log('   • Run tests: npm test');
      console.log('   • Take screenshots: npm run screenshot');  
      // Get dynamic ports for display
      const { getActivePorts } = require('../examples/server/ExampleConfigServer');
      const activePorts = await getActivePorts();
      console.log(`   • Access browser UI: http://localhost:${activePorts.http_server}`);
      console.log(`   • Use JTAG commands via WebSocket on port ${activePorts.websocket_server}`);
      process.exit(0);
    } else {
      console.log();
      console.log('💡 SUGGESTED ACTIONS:');
      console.log('   • Check logs: tmux attach-session -t jtag-test');
      console.log('   • Restart system: npm run system:restart');
      const { getActivePorts } = require('../examples/server/ExampleConfigServer');
      const activePorts = await getActivePorts();
      console.log(`   • Check port conflicts: lsof -i :${activePorts.websocket_server}`);
      process.exit(1);
    }
  } catch (error: any) {
    console.error('❌ System detector crashed:', error.message);
    process.exit(1);
  }
}

// CLI interface
if (require.main === module) {
  const args = process.argv.slice(2);
  const options: DetectorOptions = {
    critical: args.includes('--critical'),
    verbose: args.includes('--verbose')
  };

  improvedSystemDetectorV2(options).catch(error => {
    console.error('❌ System detection failed:', error);
    process.exit(1);
  });
}

export { improvedSystemDetectorV2 };