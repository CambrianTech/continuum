import { afterEach, describe, expect, it } from 'vitest';
import { pageState, type PageState } from '../../system/state/PageStateService';

describe('PageStateService', () => {
  afterEach(() => {
    pageState.clear();
  });

  it('notifies subscribers with null when page state is cleared', () => {
    const observed: Array<PageState | null> = [];

    pageState.setContent('chat', 'general', {
      id: '2789ca42-a387-43f2-815e-b0fdc60c9519',
      uniqueId: 'general',
      displayName: 'General'
    });

    const unsubscribe = pageState.subscribe((state) => {
      observed.push(state);
    });

    pageState.clear();
    unsubscribe();

    expect(observed).toHaveLength(2);
    expect(observed[0]?.contentType).toBe('chat');
    expect(observed[0]?.entityId).toBe('general');
    expect(observed[1]).toBeNull();
  });

  it('stops notifying after unsubscribe', () => {
    const observed: Array<PageState | null> = [];
    const unsubscribe = pageState.subscribe((state) => {
      observed.push(state);
    });

    unsubscribe();
    pageState.setContent('settings');
    pageState.clear();

    expect(observed).toEqual([]);
  });
});
