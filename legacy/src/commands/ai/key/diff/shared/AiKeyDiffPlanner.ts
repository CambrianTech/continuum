import { createHash } from 'node:crypto';
import type { AiKeyStatusEntry } from '../../status/shared/AiKeyStatusTypes';
import type { AiKeyDiffAction, AiKeyDiffActionType } from './AiKeyDiffTypes';

interface IndexedEntry {
  entry: AiKeyStatusEntry;
}

function entryId(entry: AiKeyStatusEntry): string {
  return `${entry.key.toUpperCase()}::${entry.provider.toLowerCase()}`;
}

function pickDisplayEntry(local: AiKeyStatusEntry | undefined, remote: AiKeyStatusEntry | undefined): AiKeyStatusEntry {
  if (local) {
    return local;
  }

  if (remote) {
    return remote;
  }

  throw new Error('AiKeyDiff planner cannot build an action without a local or remote entry');
}

function indexEntries(entries: AiKeyStatusEntry[]): Map<string, IndexedEntry> {
  const indexed = new Map<string, IndexedEntry>();

  for (const entry of entries) {
    indexed.set(entryId(entry), { entry });
  }

  return indexed;
}

function actionReason(action: AiKeyDiffActionType): string {
  switch (action) {
    case 'noop':
      return 'Both nodes report the same redacted fingerprint.';
    case 'copy-local-to-remote':
      return 'Local node is configured and remote node is missing this key.';
    case 'copy-remote-to-local':
      return 'Remote node is configured and local node is missing this key.';
    case 'conflict':
      return 'Both nodes are configured but report different redacted fingerprints.';
  }
}

function classifyAction(local?: AiKeyStatusEntry, remote?: AiKeyStatusEntry): AiKeyDiffActionType | undefined {
  const localConfigured = local?.configured === true;
  const remoteConfigured = remote?.configured === true;

  if (!localConfigured && !remoteConfigured) {
    return undefined;
  }

  if (localConfigured && remoteConfigured) {
    return local?.fingerprint === remote?.fingerprint ? 'noop' : 'conflict';
  }

  return localConfigured ? 'copy-local-to-remote' : 'copy-remote-to-local';
}

export function buildAiKeyDiffActions(
  localEntries: AiKeyStatusEntry[],
  remoteEntries: AiKeyStatusEntry[],
  targetNode?: string
): AiKeyDiffAction[] {
  const localById = indexEntries(localEntries);
  const remoteById = indexEntries(remoteEntries);
  const ids = [...new Set([...localById.keys(), ...remoteById.keys()])].sort();
  const actions: AiKeyDiffAction[] = [];

  for (const id of ids) {
    const local = localById.get(id)?.entry;
    const remote = remoteById.get(id)?.entry;
    const action = classifyAction(local, remote);

    if (!action) {
      continue;
    }

    const display = pickDisplayEntry(local, remote);
    actions.push({
      provider: display.provider,
      key: display.key,
      action,
      reason: actionReason(action),
      localConfigured: local?.configured === true,
      remoteConfigured: remote?.configured === true,
      localFingerprint: local?.fingerprint,
      remoteFingerprint: remote?.fingerprint,
      targetNode,
      requiresApproval: action !== 'noop',
    });
  }

  return actions;
}

export function createAiKeyMergePlanId(actions: AiKeyDiffAction[], targetNode?: string): string {
  const normalized = actions
    .map(action => ({
      action: action.action,
      key: action.key,
      localConfigured: action.localConfigured,
      localFingerprint: action.localFingerprint ?? '',
      provider: action.provider,
      remoteConfigured: action.remoteConfigured,
      remoteFingerprint: action.remoteFingerprint ?? '',
      targetNode: action.targetNode ?? targetNode ?? '',
    }))
    .sort((left, right) => {
      const leftId = `${left.key}:${left.provider}`;
      const rightId = `${right.key}:${right.provider}`;

      if (leftId < rightId) {
        return -1;
      }

      if (leftId > rightId) {
        return 1;
      }

      return 0;
    });

  const digest = createHash('sha256')
    .update(JSON.stringify(normalized))
    .digest('hex')
    .slice(0, 16);

  return `aikdiff_${digest}`;
}
