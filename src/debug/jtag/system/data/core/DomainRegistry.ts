/**
 * Domain Registry - Elegant Generic Type Mapping
 *
 * Uses TypeScript's type system elegantly without domain invasion.
 * Domains register themselves, registry provides generic conversion.
 */

// Generic domain factory interface
export interface DomainFactory<T> {
  fromData(data: any): T;
}

// Generic registry - collections can register any domain factory
const registry = new Map<string, DomainFactory<any>>();

/**
 * Register a domain factory for a collection
 */
export function registerDomainFactory<T>(
  collection: string,
  factory: DomainFactory<T>
): void {
  registry.set(collection, factory);
}

/**
 * Generic domain conversion with full type safety
 */
export function convertToDomainObjects<T>(
  collection: string,
  rawData: any[],
  factory?: DomainFactory<T>
): T[] {
  const domainFactory = factory || registry.get(collection);
  if (!domainFactory) {
    throw new Error(`No domain factory registered for collection: ${collection}`);
  }

  return rawData.map(data => domainFactory.fromData(data));
}

/**
 * Check if collection has domain support
 */
export function hasDomainSupport(collection: string): boolean {
  return registry.has(collection);
}

/**
 * Get registered factory (for advanced use)
 */
export function getDomainFactory<T>(collection: string): DomainFactory<T> | undefined {
  return registry.get(collection);
}