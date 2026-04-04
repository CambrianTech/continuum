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
  // Server is already ready at this point. The seed script uses ./jtag CLI
  // which connects back via WebSocket. SKIP_READINESS_CHECK bypasses the
  // seed script's own 180s readiness wait (server is already confirmed ready).
  setTimeout(async () => {
    try {
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      await execAsync('chmod +x ./jtag 2>/dev/null || true');
      // Check if rooms exist using raw jtag
      const { stdout } = await execAsync('./jtag data/list --collection=rooms --limit=1', { timeout: 15000 });
      const firstBrace = stdout.indexOf('{');
      const lastBrace = stdout.lastIndexOf('}');
      const json = firstBrace >= 0 ? JSON.parse(stdout.substring(firstBrace, lastBrace + 1)) : null;
      const hasData = json?.items?.length > 0;
      if (!hasData) {
        console.log('🌱 Empty database detected — seeding...');
        const { stdout: seedOut, stderr: seedErr } = await execAsync(
          'SKIP_READINESS_CHECK=1 npx tsx scripts/seed-continuum.ts',
          { timeout: 300000 }
        );
        if (seedOut) console.log(seedOut.slice(-2000));
        console.log('✅ Database seeded');
      } else {
        console.log(`✅ Database already seeded (${json.items.length}+ rooms)`);
      }

      // Generate avatar PNGs if missing (always runs — idempotent, skips existing)
      try {
        const { stdout: avatarOut } = await execAsync(
          'npx tsx scripts/seed/generate-avatars.ts',
          { timeout: 60000 }
        );
        if (avatarOut) console.log(avatarOut.trim());
      } catch (avatarErr: unknown) {
        const msg = avatarErr instanceof Error ? avatarErr.message : String(avatarErr);
        console.warn(`⚠️ Avatar generation: ${msg}`);
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
