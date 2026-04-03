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
  // The seed script uses ./jtag CLI which connects back to this server via WebSocket.
  // Give the WS server a moment to be fully ready before spawning the seed.
  setTimeout(async () => {
    try {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      // Make jtag executable
      await execAsync('chmod +x ./jtag 2>/dev/null || true');
      // Check if rooms exist
      const { stdout } = await execAsync('./jtag data/list --collection=rooms --limit=1');
      const parsed = JSON.parse(stdout);
      if (!parsed?.data?.length) {
        console.log('🌱 Empty database detected — seeding...');
        const { stdout: seedOut, stderr: seedErr } = await execAsync(
          'npx tsx scripts/seed-continuum.ts',
          { timeout: 180000, env: { ...process.env, PATH: process.env.PATH } }
        );
        if (seedOut) console.log(seedOut);
        if (seedErr) console.error(seedErr);
        console.log('✅ Database seeded');
      } else {
        console.log(`✅ Database already seeded (${parsed.data.length}+ rooms)`);
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      console.warn(`⚠️ Auto-seed: ${msg}`);
    }
  }, 3000);

  // Keep process alive — server event loop runs in background
}

main().catch(err => {
  console.error('❌ Fatal error:', err);
  process.exit(1);
});
