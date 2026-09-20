/**
 * Unit tests for DOMInterestRegistry — daemon-free DOM event interest tracking
 *
 * Covers register/unregister/interest-count semantics used by BaseWidget,
 * BaseContentWidget, and WidgetEventServiceBrowser, and consulted by
 * EventsDaemonBrowser for DOM CustomEvent dispatch filtering (card 5ce8f820).
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { DOMInterestRegistryImpl, domInterestRegistry } from '../../services/DOMInterestRegistry';

describe('DOMInterestRegistry', () => {
  let registry: DOMInterestRegistryImpl;

  beforeEach(() => {
    registry = new DOMInterestRegistryImpl();
  });

  it('has no interest for unregistered event names', () => {
    expect(registry.has('chat:message:received')).toBe(false);
    expect(registry.interestCount('chat:message:received')).toBe(0);
  });

  it('registers interest and reports it via has() and interestCount()', () => {
    registry.register('chat:message:received');

    expect(registry.has('chat:message:received')).toBe(true);
    expect(registry.interestCount('chat:message:received')).toBe(1);
  });

  it('reference-counts multiple registrations for the same event name', () => {
    registry.register('content:switched');
    registry.register('content:switched');
    registry.register('content:switched');

    expect(registry.interestCount('content:switched')).toBe(3);

    registry.unregister('content:switched');
    expect(registry.has('content:switched')).toBe(true);
    expect(registry.interestCount('content:switched')).toBe(2);

    registry.unregister('content:switched');
    registry.unregister('content:switched');
    expect(registry.has('content:switched')).toBe(false);
    expect(registry.interestCount('content:switched')).toBe(0);
  });

  it('returns an unregister function that releases the registration', () => {
    const unregister = registry.register('content:opened');
    expect(registry.has('content:opened')).toBe(true);

    unregister();
    expect(registry.has('content:opened')).toBe(false);
  });

  it('makes the returned unregister function idempotent (releases exactly once)', () => {
    const first = registry.register('content:closed');
    registry.register('content:closed');
    expect(registry.interestCount('content:closed')).toBe(2);

    first();
    first(); // Double-call must NOT release the second widget's registration
    expect(registry.has('content:closed')).toBe(true);
    expect(registry.interestCount('content:closed')).toBe(1);
  });

  it('treats unregister() of an unknown event name as a no-op', () => {
    expect(() => registry.unregister('never:registered')).not.toThrow();
    expect(registry.interestCount('never:registered')).toBe(0);
  });

  it('iterates registered event names for prefix matching by EventsDaemonBrowser', () => {
    registry.register('data:chat_messages');
    registry.register('content:switched');

    const names = Array.from(registry.eventNames());
    expect(names).toContain('data:chat_messages');
    expect(names).toContain('content:switched');
    expect(names).toHaveLength(2);
  });

  it('keeps event names independent of each other', () => {
    registry.register('content:opened');
    registry.register('content:closed');

    registry.unregister('content:opened');
    expect(registry.has('content:opened')).toBe(false);
    expect(registry.has('content:closed')).toBe(true);
  });

  it('exports a shared singleton instance', () => {
    expect(domInterestRegistry).toBeInstanceOf(DOMInterestRegistryImpl);
  });
});
