/**
 * LiveCallTracker - Browser-side singleton tracking active live calls.
 *
 * Tracks which entityIds have active calls AND whether media hardware
 * (mic/camera) is currently streaming. The green dot on tabs indicates
 * active media capture — a privacy indicator like Opera's tab dot.
 */

export interface LiveCallState {
  inCall: boolean;
  micActive: boolean;
  cameraActive: boolean;
}

type Listener = (calls: Map<string, LiveCallState>) => void;

const activeCalls = new Map<string, LiveCallState>();
const listeners = new Set<Listener>();

function notify(): void {
  const snapshot = new Map(activeCalls);
  for (const listener of listeners) {
    listener(snapshot);
  }
}

export const LiveCallTracker = {
  join(entityId: string): void {
    activeCalls.set(entityId, { inCall: true, micActive: false, cameraActive: false });
    notify();
  },

  leave(entityId: string): void {
    activeCalls.delete(entityId);
    notify();
  },

  /** Update media state for an active call */
  updateMedia(entityId: string, mic: boolean, camera: boolean): void {
    const state = activeCalls.get(entityId);
    if (!state) return;
    if (state.micActive === mic && state.cameraActive === camera) return; // no-op
    state.micActive = mic;
    state.cameraActive = camera;
    notify();
  },

  /** Subscribe to changes. Returns unsubscribe function. */
  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    listener(new Map(activeCalls));
    return () => listeners.delete(listener);
  },

  /** Get state for an entityId */
  get(entityId: string): LiveCallState | undefined {
    return activeCalls.get(entityId);
  }
};
