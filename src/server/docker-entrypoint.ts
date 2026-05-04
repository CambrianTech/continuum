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
import { mkdir, rm, writeFile } from 'fs/promises';
import { dirname } from 'path';

const READINESS_FILE = process.env.CONTINUUM_NODE_READY_FILE || '/root/.continuum/run/node-server.ready';

async function main(): Promise<void> {
  const activeExample = getActiveExampleName();
  const workingDir = `examples/${activeExample}`;

  console.log(`🐳 Docker node-server starting (example: ${activeExample})`);
  await rm(READINESS_FILE, { force: true });

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

  await mkdir(dirname(READINESS_FILE), { recursive: true });
  await writeFile(READINESS_FILE, `${new Date().toISOString()}\n`, 'utf8');

  console.log(`✅ Server ready (milestones: ${result.completedMilestones.join(' → ')})`);

  // Keep process alive — server event loop runs in background
}

main().catch(err => {
  console.error('❌ Fatal error:', err);
  process.exit(1);
});
