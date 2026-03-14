/**
 * ServiceInitializer — Post-daemon startup for cross-cutting services
 *
 * Services that need daemons ready but aren't daemons themselves:
 * sentinel triggers, event bridge, governance notifications, training
 * completion handler. These were incorrectly living in DataDaemonServer's
 * initializeDeferred() — they aren't data concerns.
 *
 * Called by JTAGSystem after all daemons are registered and ready.
 */

import { Logger } from '../../logging/Logger';

const log = Logger.create('ServiceInitializer');

/**
 * Background codebase indexing — runs incremental index after startup.
 * Fire-and-forget: doesn't block server startup, logs results.
 */
function initializeCodebaseIndexing(): void {
  // Delay 10s to let the system fully settle before I/O-heavy indexing
  setTimeout(async () => {
    try {
      const { getCodebaseIndexer } = await import('../../../rag/services/CodebaseIndexer');
      const indexer = getCodebaseIndexer();

      const srcRoot = process.cwd();
      log.info(`Starting background codebase indexing: ${srcRoot}`);

      const result = await indexer.index(srcRoot, { recursive: true });

      if (result.filesIndexed === 0 && result.errors.length === 0) {
        log.info('Codebase index up to date');
      } else {
        log.info(`Codebase indexed: ${result.entriesCreated} entries from ${result.filesIndexed} files in ${(result.durationMs / 1000).toFixed(1)}s`);
        if (result.errors.length > 0) {
          log.warn(`Indexing had ${result.errors.length} errors`);
        }
      }
    } catch (err) {
      log.warn(`Background codebase indexing failed: ${err}`);
    }
  }, 10_000);
}

export async function initializeServices(): Promise<void> {
  const start = Date.now();
  log.info('Initializing cross-cutting services...');

  // Governance: vote events → chat notification messages
  const { initializeGovernanceNotifications } = await import('../../../governance/GovernanceNotifications');
  initializeGovernanceNotifications();
  log.debug('Governance notifications initialized');

  // Sentinel triggers: auto-execute sentinels on event/cron/immediate
  const { initializeSentinelTriggers } = await import('../../../sentinel/SentinelTriggerService');
  await initializeSentinelTriggers();
  log.debug('Sentinel trigger service initialized');

  // Sentinel event bridge: Rust process events → TypeScript Events
  // (Still needed for genome/train TrainingCompletionHandler which subscribes to events)
  const { initializeSentinelEventBridge } = await import('../../../sentinel/SentinelEventBridge');
  initializeSentinelEventBridge();
  log.debug('Sentinel event bridge initialized');

  // Sentinel chat bridge: post sentinel lifecycle events to chat rooms
  const { initializeSentinelChatBridge } = await import('../../../sentinel/SentinelChatBridge');
  initializeSentinelChatBridge();
  log.debug('Sentinel chat bridge initialized');

  // Training completion: async genome/train → post-training workflow
  const { initializeTrainingCompletionHandler } = await import('../../../genome/server/TrainingCompletionHandler');
  initializeTrainingCompletionHandler();
  log.debug('Training completion handler initialized');

  // Codebase indexing: background incremental index so personas can answer code questions
  initializeCodebaseIndexing();

  const ms = Date.now() - start;
  log.info(`Cross-cutting services initialized (${ms}ms)`);
}
