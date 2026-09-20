/**
 * Vite Entry Point
 *
 * This is the entry point for Vite-based widget development.
 * It demonstrates the new reactive patterns that will gradually
 * replace the manual DOM manipulation in existing widgets.
 *
 * Pattern comparison:
 *
 * OLD (manual DOM):
 *   this.shadowRoot.querySelector('.count').textContent = count.toString();
 *
 * NEW (signals):
 *   const count = signal(0);
 *   effect(() => element.textContent = count.value.toString());
 */

import { signal, computed, effect } from '@preact/signals-core';

// Re-export signals for widgets to use
export { signal, computed, effect };

/**
 * Example: A reactive widget state store
 *
 * This pattern replaces scattered state management with
 * a centralized, reactive approach.
 */
export function createWidgetStore<T extends object>(initialState: T) {
  // Wrap each property in a signal
  const signals = {} as { [K in keyof T]: ReturnType<typeof signal<T[K]>> };

  for (const key in initialState) {
    signals[key] = signal(initialState[key]);
  }

  return {
    // Get current values
    get state(): T {
      const result = {} as T;
      for (const key in signals) {
        result[key] = signals[key].value;
      }
      return result;
    },

    // Update a single property
    set<K extends keyof T>(key: K, value: T[K]) {
      signals[key].value = value;
    },

    // Subscribe to changes (returns cleanup function)
    subscribe(callback: (state: T) => void): () => void {
      return effect(() => {
        callback(this.state);
      });
    },

    // Get raw signal for fine-grained reactivity
    getSignal<K extends keyof T>(key: K) {
      return signals[key];
    }
  };
}

/**
 * Example: Room Store for ChatWidget
 *
 * Demonstrates how ChatWidget state would be managed reactively:
 */
export interface RoomState {
  currentRoomId: string | null;
  roomName: string;
  messages: Array<{ id: string; content: string; author: string }>;
  isLoading: boolean;
}

export const createRoomStore = () => createWidgetStore<RoomState>({
  currentRoomId: null,
  roomName: 'General',
  messages: [],
  isLoading: false
});

/**
 * Reactive DOM binding helper
 *
 * Binds a signal to a DOM element's text content or attribute.
 * Automatically cleans up when element is removed.
 */
export function bindText<T>(
  element: Element,
  valueSignal: ReturnType<typeof signal<T>>,
  transform?: (value: T) => string
) {
  const dispose = effect(() => {
    const text = transform
      ? transform(valueSignal.value)
      : String(valueSignal.value);
    element.textContent = text;
  });

  // Return dispose function for cleanup
  return dispose;
}

/**
 * Reactive class binding helper
 *
 * Toggles a CSS class based on a boolean signal.
 */
export function bindClass(
  element: Element,
  className: string,
  conditionSignal: ReturnType<typeof signal<boolean>>
) {
  return effect(() => {
    element.classList.toggle(className, conditionSignal.value);
  });
}

// Log that Vite module loaded successfully
console.log('🚀 Vite reactive primitives loaded');
