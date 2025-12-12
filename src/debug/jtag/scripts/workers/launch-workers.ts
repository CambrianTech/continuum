#!/usr/bin/env tsx
/**
 * CLI Entry Point for Modular Worker Launcher
 *
 * USAGE:
 *   npm run worker:build     # Build all workers in parallel
 *   npm run worker:start     # Build + start all workers
 *   npm run worker:stop      # Stop all workers gracefully
 *   npm run worker:status    # Show worker status
 *   npm run worker:restart   # Restart all workers
 */

import * as path from 'path';
import { WorkerLauncher } from './WorkerLauncher';

// Parse command line arguments
const args = process.argv.slice(2);
const command = args[0] || 'start';
const verbose = args.includes('--verbose') || args.includes('-v');

// Workers directory (relative to this script)
const workersDir = path.join(__dirname, '../../workers');

async function main() {
  const launcher = new WorkerLauncher(workersDir);

  switch (command) {
    case 'build':
      console.log('📦 Building all workers...\n');
      await launcher.buildAll(verbose);
      break;

    case 'start':
      console.log('🚀 Building and starting all workers...\n');
      await launcher.buildAll(verbose);
      await launcher.startAll();

      // Print status
      console.log('\n📊 Worker Status:\n');
      const status = launcher.getStatus();
      for (const worker of status) {
        const icon = worker.running ? '✅' : '❌';
        const pid = worker.pid ? `PID: ${worker.pid}` : 'Not running';
        const uptime = worker.running ? `(${worker.uptime})` : '';
        console.log(`  ${icon} ${worker.name} - ${pid} ${uptime}`);
      }

      console.log('\n✨ All workers ready!\n');
      break;

    case 'stop':
      await launcher.stopAll();
      break;

    case 'status':
      launcher.printRegistry();

      console.log('📊 Current Status:\n');
      const currentStatus = launcher.getStatus();
      for (const worker of currentStatus) {
        const icon = worker.running ? '✅' : '❌';
        const pid = worker.pid ? `PID: ${worker.pid}` : 'Not running';
        const uptime = worker.running ? `(${worker.uptime})` : '';
        console.log(`  ${icon} ${worker.name} - ${pid} ${uptime}`);
      }
      break;

    case 'restart':
      console.log('🔄 Restarting all workers...\n');
      await launcher.stopAll();
      await new Promise(resolve => setTimeout(resolve, 1000));
      await launcher.buildAll(verbose);
      await launcher.startAll();
      console.log('\n✅ Restart complete!\n');
      break;

    case 'help':
    case '--help':
    case '-h':
      console.log(`
Modular Worker Launcher - Auto-discovery, parallel build, health monitoring

USAGE:
  npm run worker:build     Build all workers in parallel (by dependency level)
  npm run worker:start     Build + start all workers in dependency order
  npm run worker:stop      Stop all workers gracefully (reverse order)
  npm run worker:status    Show worker registry and current status
  npm run worker:restart   Stop, rebuild, and restart all workers

FLAGS:
  --verbose, -v           Show detailed build output

ADDING NEW WORKERS:
  1. Create workers/your-worker/ directory
  2. Add Cargo.toml with [[bin]] section
  3. Implement worker following LoggerWorker pattern
  4. That's it! Auto-discovered and built automatically.

DEPENDENCIES:
  Add to WorkerRegistry.ts if worker depends on others:
    dependencies: workerName === 'training' ? ['logger'] : []
      `);
      break;

    default:
      console.error(`❌ Unknown command: ${command}`);
      console.error('Run "npm run worker:launch -- help" for usage');
      process.exit(1);
  }
}

main().catch((error) => {
  console.error('❌ Fatal error:', error);
  process.exit(1);
});
