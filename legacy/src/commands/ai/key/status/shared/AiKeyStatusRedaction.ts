/**
 * Redacted API-key status helpers.
 *
 * The fingerprint is for equality checks across nodes during diff/reconcile.
 * It is intentionally short and keyed by config name, and it must never be
 * treated as a credential.
 */

import { createHash } from 'crypto';
import type { AiKeyCategory } from '../../common/AiKeyProviders';
import type { AiKeyStatusEntry } from './AiKeyStatusTypes';

export function fingerprintAiKey(keyName: string, value: string): string | undefined {
  const normalizedValue = value.trim();
  if (normalizedValue.length === 0) {
    return undefined;
  }

  return createHash('sha256')
    .update(keyName)
    .update('\0')
    .update(normalizedValue)
    .digest('hex')
    .slice(0, 16);
}

export function createAiKeyStatusEntry(data: {
  provider: string;
  key: string;
  category: AiKeyCategory;
  description: string;
  value?: string;
  processValue?: string;
}): AiKeyStatusEntry {
  const value = data.value?.trim();
  const processValue = data.processValue?.trim();
  const configuredValue = value !== undefined && value.length > 0 ? value : processValue;
  const configured = (configuredValue?.length ?? 0) > 0;

  return {
    provider: data.provider,
    key: data.key,
    category: data.category,
    description: data.description,
    configured,
    empty: !configured,
    fingerprint: configuredValue ? fingerprintAiKey(data.key, configuredValue) : undefined,
    source: value ? 'continuum-home' : processValue ? 'process-env' : 'missing'
  };
}
