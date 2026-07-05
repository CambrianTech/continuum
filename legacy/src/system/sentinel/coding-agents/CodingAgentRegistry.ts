/**
 * CodingAgentRegistry — dynamic provider registry.
 *
 * Providers self-register at import time. No switch statements, no enums,
 * no central list. Adding a provider = implement interface + register().
 */

import type { CodingAgentProvider } from './CodingAgentProvider';

class CodingAgentRegistryImpl {
  private readonly _providers = new Map<string, CodingAgentProvider>();

  register(provider: CodingAgentProvider): void {
    this._providers.set(provider.providerId, provider);
  }

  get(providerId: string): CodingAgentProvider | undefined {
    return this._providers.get(providerId);
  }

  has(providerId: string): boolean {
    return this._providers.has(providerId);
  }

  get providerIds(): string[] {
    return Array.from(this._providers.keys());
  }

  get providers(): CodingAgentProvider[] {
    return Array.from(this._providers.values());
  }
}

export const CodingAgentRegistry = new CodingAgentRegistryImpl();
