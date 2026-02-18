/**
 * AdapterRegistry - Global registry for all LoRA adapters
 *
 * Tracks all registered adapters across all personas.
 * Used by GenomeDaemon for global coordination.
 *
 * Phase 7: Mock adapters only
 * Phase 8+: Real Candle/PEFT adapters
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { MockLoRAAdapter } from './MockLoRAAdapter';

/**
 * Adapter metadata for registry
 */
export interface AdapterMetadata {
  id: UUID;
  name: string;
  domain: string;
  sizeMB: number;
  priority: number;
  path?: string;  // File path (for real adapters in Phase 8+)
}

/**
 * AdapterRegistry - Global in-memory registry
 *
 * Simple Map-based storage for looking up adapters by ID or name.
 * No persistence (ephemeral per server lifetime).
 */
export class AdapterRegistry {
  private adapters: Map<UUID, MockLoRAAdapter> = new Map();
  private nameIndex: Map<string, UUID> = new Map();

  /**
   * Register a new adapter
   *
   * @throws Error if adapter with same ID or name already exists
   */
  register(adapter: MockLoRAAdapter): void {
    const id = adapter.getId();
    const name = adapter.getName();

    if (this.adapters.has(id)) {
      throw new Error(`Adapter with ID ${id} is already registered`);
    }

    if (this.nameIndex.has(name)) {
      throw new Error(`Adapter with name "${name}" is already registered`);
    }

    this.adapters.set(id, adapter);
    this.nameIndex.set(name, id);

    console.log(`🧬 AdapterRegistry: Registered ${name} (${id})`);
  }

  /**
   * Get adapter by ID
   *
   * @returns Adapter or undefined if not found
   */
  getById(id: UUID): MockLoRAAdapter | undefined {
    return this.adapters.get(id);
  }

  /**
   * Get adapter by name
   *
   * @returns Adapter or undefined if not found
   */
  getByName(name: string): MockLoRAAdapter | undefined {
    const id = this.nameIndex.get(name);
    if (!id) return undefined;
    return this.adapters.get(id);
  }

  /**
   * Check if adapter exists by ID
   */
  hasId(id: UUID): boolean {
    return this.adapters.has(id);
  }

  /**
   * Check if adapter exists by name
   */
  hasName(name: string): boolean {
    return this.nameIndex.has(name);
  }

  /**
   * List all registered adapters
   *
   * @returns Array of all adapters
   */
  listAll(): MockLoRAAdapter[] {
    return Array.from(this.adapters.values());
  }

  /**
   * List adapters by domain
   *
   * @param domain Domain to filter by (e.g., 'knowledge', 'personality')
   * @returns Array of adapters in domain
   */
  listByDomain(domain: string): MockLoRAAdapter[] {
    return Array.from(this.adapters.values()).filter(
      adapter => adapter.getDomain() === domain
    );
  }

  /**
   * Get adapter metadata (for serialization/inspection)
   */
  getMetadata(id: UUID): AdapterMetadata | undefined {
    const adapter = this.adapters.get(id);
    if (!adapter) return undefined;

    return {
      id: adapter.getId(),
      name: adapter.getName(),
      domain: adapter.getDomain(),
      sizeMB: adapter.getSize(),
      priority: adapter.getPriority()
    };
  }

  /**
   * Unregister adapter by ID
   *
   * @throws Error if adapter is currently loaded
   */
  unregister(id: UUID): void {
    const adapter = this.adapters.get(id);
    if (!adapter) {
      throw new Error(`Adapter with ID ${id} is not registered`);
    }

    if (adapter.isLoaded()) {
      throw new Error(`Cannot unregister loaded adapter ${adapter.getName()}`);
    }

    const name = adapter.getName();
    this.adapters.delete(id);
    this.nameIndex.delete(name);

    console.log(`🧬 AdapterRegistry: Unregistered ${name} (${id})`);
  }

  /**
   * Get total count of registered adapters
   */
  count(): number {
    return this.adapters.size;
  }

  /**
   * Clear all adapters (for testing)
   *
   * @throws Error if any adapters are currently loaded
   */
  clear(): void {
    const loadedAdapters = Array.from(this.adapters.values()).filter(
      a => a.isLoaded()
    );

    if (loadedAdapters.length > 0) {
      throw new Error(
        `Cannot clear registry with ${loadedAdapters.length} loaded adapters`
      );
    }

    this.adapters.clear();
    this.nameIndex.clear();

    console.log('🧬 AdapterRegistry: Cleared all adapters');
  }
}
