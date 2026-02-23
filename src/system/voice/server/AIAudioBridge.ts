/**
 * AIAudioBridge - Routes AI persona speech to LiveKit via Rust IPC
 *
 * ARCHITECTURE (post-LiveKit migration):
 * - Speaking: Rust voice/speak-in-call synthesizes TTS + publishes via LiveKit agent
 * - LiveKitAgentManager creates agents on-demand (no pre-connection needed)
 * - TypeScript only coordinates (who speaks when) and emits events for UI
 *
 * Audio NEVER leaves Rust. One IPC call does synthesis + LiveKit publish.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import { Events } from '../../core/shared/Events';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { EVENT_SCOPES } from '../../events/shared/EventSystemConstants';

/** Tracks AI participants registered in calls */
interface AIParticipant {
  callId: string;
  userId: string;
  displayName: string;
}

export class AIAudioBridge {
  private static _instance: AIAudioBridge | null = null;
  /** Registered AI participants, keyed by `${callId}-${userId}` */
  private participants: Map<string, AIParticipant> = new Map();
  private ipcClient: RustCoreIPCClient;

  private constructor() {
    this.ipcClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
    this.ipcClient.connect().catch(() => {});
  }

  static get instance(): AIAudioBridge {
    if (!AIAudioBridge._instance) {
      AIAudioBridge._instance = new AIAudioBridge();
    }
    return AIAudioBridge._instance;
  }

  /**
   * Register an AI participant for a voice call.
   * LiveKitAgentManager creates the actual LiveKit agent on-demand when speak() is called.
   */
  async joinCall(callId: string, userId: UUID, displayName: string): Promise<boolean> {
    const key = `${callId}-${userId}`;

    if (this.participants.has(key)) {
      return true;
    }

    this.participants.set(key, { callId, userId, displayName });
    return true;
  }

  /**
   * Unregister AI from a voice call
   */
  leaveCall(callId: string, userId: UUID): void {
    const key = `${callId}-${userId}`;
    const participant = this.participants.get(key);
    if (participant) {
      this.participants.delete(key);
    }
  }

  /**
   * Speak in a call — synthesizes TTS and publishes audio via LiveKit.
   *
   * One Rust IPC call: voice/speak-in-call → TTS synthesis → LiveKitAgent.speak()
   * Audio stays in Rust. LiveKitAgentManager creates agents on-demand.
   *
   * @param voice - Voice identifier for TTS. Named voice ("alba") or any string (hashed to pick voice).
   */
  async speak(callId: string, userId: UUID, text: string, voice?: string): Promise<void> {
    const key = `${callId}-${userId}`;
    const participant = this.participants.get(key);
    const displayName = participant?.displayName || userId.slice(0, 8);

    try {
      // Reconnect IPC if needed (Rust worker may have restarted)
      if (!this.ipcClient.connected) {
        await this.ipcClient.connect();
      }

      const voiceId = voice ?? userId;

      // ONE Rust IPC call: synthesize + publish via LiveKit agent.
      // LiveKitAgentManager.speak_in_call() creates agent on-demand if needed.
      // Kokoro: local ONNX, 54 voices, fast reliable — until Orpheus (3B, LoRA-trainable) is installed.
      const result = await this.ipcClient.voiceSpeakInCall(callId, userId, text, voiceId, 'kokoro');

      const audioDurationMs = result.durationMs;
      await this.emitSpeechEvent(callId, userId, displayName, text, audioDurationMs, false);

    } catch {
      await this.emitSpeechEvent(callId, userId, displayName, text, 0, true);
    }
  }

  /**
   * Emit voice:ai:speech event for VoiceOrchestrator coordination + browser UI.
   * ALWAYS emits on both success and failure — without this, the voice chain breaks.
   */
  private async emitSpeechEvent(
    callId: string,
    userId: UUID,
    displayName: string,
    text: string,
    audioDurationMs: number,
    failed: boolean
  ): Promise<void> {
    const event = {
      sessionId: callId,
      speakerId: userId,
      speakerName: displayName,
      text,
      audioDurationMs,
      failed,
      timestamp: Date.now()
    };

    if (DataDaemon.jtagContext) {
      await Events.emit(DataDaemon.jtagContext, 'voice:ai:speech', event, { scope: EVENT_SCOPES.GLOBAL });
    } else {
      Events.emit('voice:ai:speech', event);
    }
  }

  /**
   * Check if AI is registered for a call.
   * With LiveKit, agents are created on-demand — registration is sufficient.
   */
  isInCall(callId: string, userId: UUID): boolean {
    const key = `${callId}-${userId}`;
    return this.participants.has(key);
  }

  /**
   * Get all AI participants in a call
   */
  getAIParticipants(callId: string): string[] {
    const participants: string[] = [];
    for (const [key, p] of this.participants) {
      if (key.startsWith(callId)) {
        participants.push(p.displayName);
      }
    }
    return participants;
  }
}

// Singleton accessor
export function getAIAudioBridge(): AIAudioBridge {
  return AIAudioBridge.instance;
}
