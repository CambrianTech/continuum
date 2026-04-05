/**
 * Docker Entry Point — starts the node server without browser.
 *
 * Used by the node-server Docker container:
 *   CMD ["node", "dist/server/docker-entrypoint.js"]
 *
 * Uses the SystemOrchestrator's 'cli-command' entry point which
 * starts the HTTP/WS server and all daemons, but skips browser detection.
 */

import { systemOrchestrator } from '../system/orchestration/SystemOrchestrator';
import { getActiveExampleName } from '../examples/server/ExampleConfigServer';

async function main(): Promise<void> {
  const activeExample = getActiveExampleName();
  const workingDir = `examples/${activeExample}`;

  console.log(`🐳 Docker node-server starting (example: ${activeExample})`);

  const result = await systemOrchestrator.orchestrate('cli-command', {
    workingDir,
    skipBrowser: true,
    verbose: true,
  });

  if (!result.success) {
    console.error(`❌ Server startup failed at milestone: ${result.failedMilestone}`);
    console.error(`❌ Error: ${result.error}`);
    process.exit(1);
  }

  console.log(`✅ Server ready (milestones: ${result.completedMilestones.join(' → ')})`);

  // Auto-seed database if empty (first run).
  // In-process via Commands.execute() — zero subprocess spawns.
  // ~200MB instead of 2GB, <5 seconds instead of 30+.
  setTimeout(async () => {
    try {
      const { seedDatabase } = await import('./seed-in-process');
      const seeded = await seedDatabase();
      if (seeded) {
        console.log('✅ Database seeded');
      } else {
        console.log('✅ Database already seeded');
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      console.warn(`⚠️ Auto-seed: ${msg}`);
    }
  }, 5000);

  // Keep process alive — server event loop runs in background
}

main().catch(err => {
  console.error('❌ Fatal error:', err);
  process.exit(1);
});
