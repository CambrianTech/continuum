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

  // Seed BEFORE declaring the server ready. Old code fired auto-seed
  // via setTimeout(5000) and swallowed errors to console.warn — health
  // probes returned 200 before any room/persona existed, so chat/send
  // probes hit "Room not found: general" silently. Carl-install-smoke
  // caught this on PR #1038. Now seed is a blocking milestone: server
  // ready ≡ rooms + personas exist. Seed errors propagate to exit 1.
  try {
    const { seedDatabase } = await import('./seed-in-process');
    const seeded = await seedDatabase();
    console.log(seeded ? '✅ Database seeded' : '✅ Database already seeded');
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error(`❌ Auto-seed FAILED: ${msg}`);
    console.error('   Server cannot serve chat without seeded rooms/personas. Exiting.');
    process.exit(1);
  }

  console.log(`✅ Server ready (milestones: ${result.completedMilestones.join(' → ')} → seed)`);

  // Keep process alive — server event loop runs in background
}

main().catch(err => {
  console.error('❌ Fatal error:', err);
  process.exit(1);
});
