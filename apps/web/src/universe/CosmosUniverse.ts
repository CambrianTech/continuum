/**
 * `CosmosUniverse` — the room reflected in a night sky: citizens as named
 * stars on a quiet ring.
 *
 * Adapter over the `<cosmos-backdrop>` element: the CLASS owns the element
 * and translates the [`UniverseInstance`] contract onto it — the host never
 * touches the element's properties ([[compression]]: one contract, any
 * implementation shape behind it).
 */

import '../render/CosmosBackdrop';
import { CosmosBackdrop } from '../render/CosmosBackdrop';
import type { Universe, UniverseFact, UniverseFrame, UniverseInstance } from './Universe';

class CosmosInstance implements UniverseInstance {
  readonly element: HTMLElement;
  private readonly backdrop: CosmosBackdrop;

  constructor() {
    this.backdrop = new CosmosBackdrop();
    this.element = this.backdrop;
  }

  update(frame: UniverseFrame): void {
    this.backdrop.citizens = frame.citizens.map((c) => ({ name: c.name, active: c.active }));
  }

  onFact(_fact: UniverseFact): void {}

  dispose(): void {
    this.backdrop.remove();
  }
}

export class CosmosUniverse implements Universe {
  readonly key = 'cosmos';

  mount(): UniverseInstance {
    return new CosmosInstance();
  }
}
