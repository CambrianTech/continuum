/**
 * LiveWidget - Real-time collaboration orchestrator (audio, video, screen share)
 *
 * Like Slack huddles / Discord voice channels / Zoom calls.
 * Can be attached to any activity/room as a modality layer.
 *
 * Architecture:
 * - Orchestrator: owns session lifecycle, AudioStreamClient, state persistence
 * - Delegates rendering to sub-components:
 *   - <live-participant-tile> for each participant (owns video container in shadow DOM)
 *   - <live-controls> for media buttons (fires events back up)
 *   - <live-captions> for transcription overlay (manages its own fade timers)
 */

import { ReactiveWidget, html, reactive, unsafeCSS, type TemplateResult, type CSSResultGroup } from '../shared/ReactiveWidget';
import { repeat } from 'lit/directives/repeat.js';
import { ref, createRef } from 'lit/directives/ref.js';
import { styles as LIVE_STYLES } from './public/live-widget.styles';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { COMMANDS } from '../../shared/generated-command-constants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { LiveJoinParams, LiveJoinResult } from '../../commands/collaboration/live/join/shared/LiveJoinTypes';
import type { LiveLeaveParams, LiveLeaveResult } from '../../commands/collaboration/live/leave/shared/LiveLeaveTypes';
import type { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import type { CallEntity, CallParticipant } from '../../system/data/entities/CallEntity';
import { AudioStreamClient, type TranscriptionResult } from './AudioStreamClient';
import { LiveCallTracker } from './LiveCallTracker';
import { ContentService } from '../../system/state/ContentService';
import { contentState } from '../../system/state/ContentStateService';
import { ContentLifecycle, type ContentLifecycleParticipant } from '../../system/state/ContentLifecycle';
import { AI_DECISION_EVENTS } from '../../system/events/shared/AIDecisionEvents';

import { DataUpdate } from '../../commands/data/update/shared/DataUpdateTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
// Import sub-components (registers custom elements)
import './LiveParticipantTile';
import './LiveControls';
import './LiveCaptions';
import type { LiveControls } from './LiveControls';
import type { LiveCaptions } from './LiveCaptions';

type ActivityState = 'thinking' | 'generating' | 'using-tool' | null;

interface Participant {
  userId: string;
  displayName: string;
  avatar?: string;
  micEnabled: boolean;
  cameraEnabled: boolean;
  screenShareEnabled: boolean;
  isSpeaking: boolean;
  activityState: ActivityState;
}

interface TranscriptEntry {
  speakerName: string;
  text: string;
  timestamp: number;
}

// Constants
const DEFAULT_AI_SPEECH_DURATION_MS = 5000;
const CAPTION_EXTEND_BUFFER_MS = 500;
const TILE_RESOLUTION_DEBOUNCE_MS = 1000;

export class LiveWidget extends ReactiveWidget implements ContentLifecycleParticipant {
  // Session state
  @reactive() private sessionId: string | null = null;
  @reactive() private participants: Participant[] = [];
  @reactive() private isJoined: boolean = false;
  @reactive() private isPreview: boolean = false;
  @reactive() private previewStream: MediaStream | null = null;

  // User intent — persisted preferences, always shown in controls.
  // These represent what the user WANTS, not what LiveKit currently has.
  @reactive() private _micIntent: boolean = true;
  @reactive() private _speakerIntent: boolean = true;
  @reactive() private speakerVolume: number = 1.0;
  @reactive() private cameraEnabled: boolean = false;
  @reactive() private screenShareEnabled: boolean = false;
  @reactive() private micPermissionGranted: boolean = false;
  @reactive() private cameraPermissionGranted: boolean = false;
  @reactive() private captionsEnabled: boolean = true;

  // Transcript panel
  @reactive() private _transcriptOpen: boolean = false;
  private _transcript: TranscriptEntry[] = [];

  // Transient conditions — affect effective media state, NEVER persisted.
  // Tab switches and visibility changes mute media without touching user intent.
  @reactive() private _tabActive: boolean = true;
  @reactive() private _widgetVisible: boolean = true;

  // Entity association
  @reactive() private entityId: string = '';

  // Spotlight mode
  @reactive() private spotlightUserId: string | null = null;
  /** True when spotlight was set by user click (manual pin). Auto-spotlight won't override. */
  private _spotlightPinned: boolean = false;

  // Set of participant userIds with active video
  @reactive() private activeVideoUsers: Set<string> = new Set();

  // Media streams
  private localStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;

  // Visibility observer for auto-mute
  private visibilityObserver: IntersectionObserver | null = null;

  // Audio streaming client (WebSocket to Rust call server)
  private audioClient: AudioStreamClient | null = null;

  // Event subscriptions
  private unsubscribers: Array<() => void> = [];

  // Remote video elements from LiveKit — keyed by participant identity (userId)
  private _remoteVideoElements: Map<string, HTMLVideoElement> = new Map();

  // Typed refs to child components (Lit ref directive — no querySelector)
  private _captionsRef = createRef<LiveCaptions>();
  private _controlsRef = createRef<LiveControls>();
  private _transcriptBodyRef = createRef<HTMLDivElement>();
  private _transcriptScrollTop: number = 0;

  // Speaking state is driven by LiveKit ActiveSpeakersChanged
  // Spotlight hold: keep current speaker spotlighted through brief pauses
  private _spotlightHoldTimer: ReturnType<typeof setTimeout> | null = null;
  private static readonly SPOTLIGHT_HOLD_MS = 1500; // Hold spotlight 1.5s after speaker goes silent

  /** Effective mic state: user intent AND permission granted AND all transient conditions satisfied */
  private get _effectiveMic(): boolean {
    return this._micIntent && this.micPermissionGranted && this._tabActive && this._widgetVisible;
  }

  /** Effective speaker state: user intent AND all transient conditions satisfied */
  private get _effectiveSpeaker(): boolean {
    return this._speakerIntent && this._tabActive && this._widgetVisible;
  }


  // Track last applied state to deduplicate LiveKit calls
  private _lastAppliedMic: boolean | null = null;
  private _lastAppliedSpeaker: boolean | null = null;

  // Keyboard listener for Escape key
  private _escapeHandler = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && this.spotlightUserId) {
      this.spotlightUserId = null;
      this._spotlightPinned = false;
    }
  };

  // Tile resolution tracking — collected from tile-resized events, batched to data channel
  private _tileResolutions: Map<string, { width: number; height: number }> = new Map();
  private _tileResDebounce: ReturnType<typeof setTimeout> | null = null;

  // Reentrancy guard for async mic state application
  private _applyingMicState = false;

  // Reentrancy guard for handleJoin (async — prevents duplicate joins on rapid refresh)
  private _joining = false;

  // Page unload handler (must be stored for removeEventListener)
  private _unloadHandler: (() => void) | null = null;

  // State loading tracking
  @reactive() private _stateLoaded: boolean = false;
  private stateLoadedPromise: Promise<void> | null = null;

  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(LIVE_STYLES)
  ] as CSSResultGroup;

  // ========================================
  // Lifecycle
  // ========================================

  override connectedCallback(): void {
    super.connectedCallback();

    document.addEventListener('keydown', this._escapeHandler);

    // Best-effort leave on page unload (refresh, close, navigate away).
    // May not complete — server-side rejoin cleanup handles the rest.
    this._unloadHandler = () => {
      if (this.isJoined && this.sessionId) {
        Commands.execute(COMMANDS.COLLABORATION_LIVE_LEAVE, {
          sessionId: this.sessionId
        }).catch(() => {});
      }
    };
    window.addEventListener('beforeunload', this._unloadHandler);

    this.stateLoadedPromise = this.loadUserContext().then(() => {
      this.loadCallState();
      this._stateLoaded = true;
      console.log(`LiveWidget: State loaded (mic=${this._micIntent}, speaker=${this._speakerIntent}, captions=${this.captionsEnabled})`);
      this.requestUpdate();
    }).catch(err => {
      console.error('LiveWidget: Failed to load user context:', err);
      this._stateLoaded = true; // Unblock UI — use defaults
    });

    // Visibility observer: just set the transient flag. The effective state
    // computation handles muting — no save/restore dance needed.
    this.visibilityObserver = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        this._widgetVisible = entry.isIntersecting;
      }
    }, { threshold: 0.1 });

    this.visibilityObserver.observe(this);

    // ContentLifecycle registration happens in onActivate/setEntityId
    // when entityId is actually known (it's empty at connectedCallback time).
  }

  /** Save transcript scroll position before Lit re-renders */
  protected override willUpdate(_changedProperties: Map<string, unknown>): void {
    super.willUpdate(_changedProperties);
    const body = this._transcriptBodyRef.value;
    if (body) {
      this._transcriptScrollTop = body.scrollTop;
    }
  }

  protected override updated(_changedProperties: Map<string, unknown>): void {
    super.updated(_changedProperties);
    this._syncMediaState();
    // Restore transcript scroll position after re-render
    const body = this._transcriptBodyRef.value;
    if (body && this._transcriptScrollTop > 0) {
      body.scrollTop = this._transcriptScrollTop;
    }
    // Update tab indicator with actual streaming state (not just permission)
    if (this.isJoined && this.entityId) {
      LiveCallTracker.updateMedia(
        this.entityId,
        this._effectiveMic,
        this.cameraEnabled && this.cameraPermissionGranted
      );
    }
  }

  /**
   * Sync effective media state to LiveKit. Safe to call frequently — deduplicates
   * by comparing effective state against last-applied state.
   *
   * CRITICAL: Must check isConnected, not just audioClient existence.
   * Between audioClient creation and room.connect() completion, applyMicState()
   * early-returns without setting _lastAppliedMic, causing the .finally()
   * re-check to recurse infinitely (microtask flood → browser hang).
   */
  private _syncMediaState(): void {
    if (!this.audioClient?.isConnected) return;

    // Speaker is synchronous — apply whenever effective state or volume changes
    const effectiveSpeaker = this._effectiveSpeaker;
    if (effectiveSpeaker !== this._lastAppliedSpeaker) {
      this._lastAppliedSpeaker = effectiveSpeaker;
      this.applySpeakerState();
    }

    // Mic is async — guard against concurrent LiveKit calls
    const effectiveMic = this._effectiveMic;
    if (effectiveMic !== this._lastAppliedMic && !this._applyingMicState) {
      this._applyingMicState = true;
      this.applyMicState().finally(() => {
        this._applyingMicState = false;
        // Re-check: effective state may have changed during async apply
        if (this._effectiveMic !== this._lastAppliedMic) {
          this._syncMediaState();
        }
      });
    }
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    document.removeEventListener('keydown', this._escapeHandler);
    if (this._unloadHandler) {
      window.removeEventListener('beforeunload', this._unloadHandler);
      this._unloadHandler = null;
    }

    // Unregister from content lifecycle
    if (this.entityId) {
      ContentLifecycle.unregister(this.entityId, this);
    }

    // If still joined when DOM removes us (e.g. page navigation without tab close),
    // do a best-effort cleanup. Normally willClose() already handled this.
    if (this.isJoined) {
      this.cleanup();
    }
  }

  // ========================================
  // Activation / Deactivation (MainWidget interface)
  // ========================================

  onActivate(entityId?: string, metadata?: unknown): void {
    this._tabActive = true; // Effective state recomputes — mic/speaker restore automatically

    if (entityId) {
      // Unregister old entityId if switching
      if (this.entityId) {
        ContentLifecycle.unregister(this.entityId, this);
      }

      const cleanEntityId = entityId.startsWith('live-') ? entityId.slice(5) : entityId;
      this.entityId = cleanEntityId;

      const meta = metadata as { room?: { id?: string; displayName?: string }; session?: { id: string } } | undefined;
      if (meta?.room?.id) {
        this.entityId = meta.room.id;
      }

      // Register for content lifecycle now that entityId is known
      ContentLifecycle.register(this.entityId, this);

      if (!this.isJoined) {
        console.log('LiveWidget: Auto-joining for entityId:', this.entityId);
        this.handleJoin();
      }
    }
  }

  onDeactivate(): void {
    this._tabActive = false; // Effective state recomputes — mic/speaker mute automatically
  }

  setEntityId(entityId: string): void {
    if (this.entityId) {
      ContentLifecycle.unregister(this.entityId, this);
    }
    this.entityId = entityId;
    if (entityId) {
      ContentLifecycle.register(entityId, this);
    }
  }

  // ========================================
  // State Persistence
  // ========================================

  private loadCallState(): void {
    const callState = this.userState?.callState;
    if (callState) {
      this._micIntent = callState.micEnabled ?? true;
      this._speakerIntent = callState.speakerEnabled ?? true;
      this.speakerVolume = callState.speakerVolume ?? 1.0;
      this.cameraEnabled = callState.cameraEnabled ?? false;
      this.screenShareEnabled = callState.screenShareEnabled ?? false;
      this.captionsEnabled = callState.captionsEnabled ?? true;
    }
  }

  private async saveCallState(): Promise<void> {
    if (!this.userState?.id) {
      console.warn('LiveWidget: Cannot save call state - userState not loaded');
      return;
    }

    const newCallState = {
      micEnabled: this._micIntent,
      speakerEnabled: this._speakerIntent,
      speakerVolume: this.speakerVolume,
      cameraEnabled: this.cameraEnabled,
      screenShareEnabled: this.screenShareEnabled,
      captionsEnabled: this.captionsEnabled,
      currentCallId: this.sessionId || undefined
    };

    try {
      const result = await DataUpdate.execute<UserStateEntity>({
        collection: 'user_states',
        id: this.userState.id as UUID,
        data: { callState: newCallState },
        dbHandle: 'default'
      });

      if (!result.success) {
        console.error('LiveWidget: Failed to save call state:', result.error);
      } else {
        console.log('LiveWidget: Call state saved:', newCallState);
      }
    } catch (error) {
      console.error('LiveWidget: Exception saving call state:', error);
    }
  }

  // ========================================
  // Session Lifecycle
  // ========================================

  private async handleJoin(): Promise<void> {
    if (this._joining || this.isJoined) return;
    if (!this.entityId) {
      console.error('LiveWidget: No entityId specified');
      return;
    }
    this._joining = true;

    try {
      await this._executeJoin();
    } finally {
      this._joining = false;
    }
  }

  /** Inner join logic — separated from guard for clarity */
  private async _executeJoin(): Promise<void> {
    if (this.stateLoadedPromise) {
      await this.stateLoadedPromise;
    }

    if (!this.currentUser?.id) {
      console.warn('LiveWidget: No user identity — retrying state load...');
      try { await this.loadUserContext(); } catch (_) { /* ignore */ }
    }
    if (!this.currentUser?.id) {
      console.error('LiveWidget: Cannot join — no user identity after retry');
      return;
    }

    this.isPreview = false;

    try {
      const userId = this.currentUser?.id;
      console.log('LiveWidget: Joining with entityId:', this.entityId, 'userId:', userId);
      const result = await Commands.execute<LiveJoinParams, LiveJoinResult>(COMMANDS.COLLABORATION_LIVE_JOIN, {
        entityId: this.entityId,
      });

      if (result.success && result.callId) {
        this.sessionId = result.callId;
        this.isJoined = true;
        this.loadCallState();
        this._stateLoaded = true;
        LiveCallTracker.join(this.entityId);

        // Start with just the local user. Remote participants are added
        // incrementally via LiveKit's ParticipantConnected events as agents
        // connect (staggered 2s apart). This prevents the grid from showing
        // a large initial set then fluctuating as connections stabilize.
        const myInfo = result.myParticipant;
        this.participants = [{
          userId: myInfo?.userId || ('self' as UUID),
          displayName: myInfo?.displayName || 'You',
          micEnabled: true,
          cameraEnabled: false,
          screenShareEnabled: false,
          isSpeaking: false,
          activityState: null,
        }];
        console.log('LiveWidget: Starting with self, agents will appear as they connect');
        this.requestUpdate();

        this.subscribeToEvents();

        this.audioClient = new AudioStreamClient({
          onMicLevel: (level) => {
            // Route mic level to the controls component
            const controls = this._controlsRef.value ?? null;
            controls?.setMicLevel(level);
          },
          onParticipantJoined: (userId, displayName) => {
            console.log(`LiveWidget: ${displayName} joined the call (total before: ${this.participants.length})`);
            if (!this.participants.find(p => p.userId === userId)) {
              this.participants = [...this.participants, {
                userId: userId as UUID,
                displayName,
                micEnabled: true,
                cameraEnabled: false,
                screenShareEnabled: false,
                isSpeaking: false,
                activityState: null,
              }];
              console.log(`LiveWidget: Added ${displayName}, total now: ${this.participants.length}`);
            } else {
              console.log(`LiveWidget: ${displayName} already in list, skipping`);
            }
          },
          onParticipantLeft: (userId) => {
            // DON'T remove from participant grid on LiveKit disconnect.
            // Agents connect with 2s stagger and may have temporary connection
            // issues. Removing them causes the grid to fluctuate wildly
            // (15→10→12→8→15). The server's initial list is the ground truth
            // for who should be in the call. Just clean up media resources.
            console.log(`LiveWidget: ${userId} disconnected (keeping in grid)`);
            this._remoteVideoElements.delete(userId);
            const newSet = new Set(this.activeVideoUsers);
            newSet.delete(userId);
            this.activeVideoUsers = newSet;
          },
          onConnectionChange: (connected) => {
            console.log(`LiveWidget: Audio stream ${connected ? 'connected' : 'disconnected'}`);
            if (connected) {
              // Re-apply full mic+speaker state through canonical path (handles
              // reconnections too). Don't call setMuted() directly — that bypasses
              // startMicrophone/stopMicrophone and mic level monitoring setup.
              this.applyMicState();
              this.applySpeakerState();
            }
          },
          onVideoTrackAdded: (participantId: string, element: HTMLVideoElement) => {
            console.log(`LiveWidget: Video track added for ${participantId}`);
            this._remoteVideoElements.set(participantId, element);
            this.activeVideoUsers = new Set([...this.activeVideoUsers, participantId]);
          },
          onVideoTrackRemoved: (participantId: string) => {
            console.log(`LiveWidget: Video track removed for ${participantId}`);
            this._remoteVideoElements.delete(participantId);
            const newSet = new Set(this.activeVideoUsers);
            newSet.delete(participantId);
            this.activeVideoUsers = newSet;
          },
          onActiveSpeakersChanged: (speakerIds: string[]) => {
            // Ground truth speaking detection — based on ACTUAL audio levels
            // at the browser, not timers. LiveKit detects audio energy from
            // both human mics and AI TTS streams with built-in hysteresis.
            const speakerSet = new Set(speakerIds);
            this.participants = this.participants.map(p => ({
              ...p,
              isSpeaking: speakerSet.has(p.userId)
            }));

            // Auto-spotlight with hold timer: keep current speaker through brief pauses.
            // Without this, natural breath pauses cause rapid grid↔spotlight flipping.
            if (!this._spotlightPinned) {
              const aiSpeaker = speakerIds.find(id => id !== this.currentUser?.id);
              if (aiSpeaker) {
                // New speaker detected — switch immediately, cancel any pending fade-out
                if (this._spotlightHoldTimer) {
                  clearTimeout(this._spotlightHoldTimer);
                  this._spotlightHoldTimer = null;
                }
                this.spotlightUserId = aiSpeaker as UUID;
              } else if (speakerIds.length === 0 && this.spotlightUserId) {
                // No one speaking — hold spotlight briefly before returning to grid.
                // This prevents flicker during natural speech pauses.
                if (!this._spotlightHoldTimer) {
                  this._spotlightHoldTimer = setTimeout(() => {
                    this._spotlightHoldTimer = null;
                    if (!this._spotlightPinned) {
                      this.spotlightUserId = null;
                    }
                  }, LiveWidget.SPOTLIGHT_HOLD_MS);
                }
              }
            }

            this.requestUpdate();
          },
          onTranscription: async (transcription: TranscriptionResult) => {
            if (!this.sessionId) return;

            const resolvedName = this.participants.find(p => p.userId === transcription.userId)?.displayName
              || transcription.displayName;

            // Always log to transcript (even when captions hidden)
            this._appendTranscript(resolvedName, transcription.text);

            // Show caption overlay only when enabled.
            // AI routing is handled server-side: Rust STT listener calls
            // collaboration/live/transcription directly via IPC. DO NOT relay from
            // browser — that caused every transcription to reach AIs twice.
            if (this.captionsEnabled) {
              const captions = this._captionsRef.value ?? null;
              captions?.setCaption(resolvedName, transcription.text);
            }
          },
        });

        try {
          const myUserId = result.myParticipant?.userId || 'unknown';
          const myDisplayName = result.myParticipant?.displayName || 'Unknown User';

          await this.audioClient.join(result.callId, myUserId, myDisplayName, result.livekitUrl, result.livekitToken);
          console.log('LiveWidget: Connected to audio stream');

          // Merge participants already in the LiveKit room into our grid.
          // ParticipantConnected events handle incremental arrivals, but for
          // participants already present when we connect, we enumerate them here.
          const existing = this.audioClient.getConnectedParticipants();
          if (existing.length > 0) {
            const currentIds = new Set(this.participants.map(p => p.userId));
            const newParticipants = [...this.participants];
            for (const ep of existing) {
              if (!currentIds.has(ep.userId as UUID)) {
                newParticipants.push({
                  userId: ep.userId as UUID,
                  displayName: ep.displayName,
                  micEnabled: true,
                  cameraEnabled: false,
                  screenShareEnabled: false,
                  isSpeaking: false,
                  activityState: null,
                });
              }
            }
            this.participants = newParticipants;
            console.log(`LiveWidget: Added ${existing.length} existing participants (total: ${this.participants.length})`);
          }

          await this.applyMicState();
          this.applySpeakerState();
          console.log(`LiveWidget: State applied from saved (mic=${this._micIntent}, speaker=${this._speakerIntent}, volume=${this.speakerVolume})`);
        } catch (audioError) {
          console.warn('LiveWidget: Audio stream failed:', audioError);
        }
      }
    } catch (error) {
      console.error('LiveWidget: Failed to join:', error);
    }
  }

  /**
   * User clicked "leave" or "end call" — close the content tab.
   * ContentService.close() → ContentLifecycle.willClose() → cleanup.
   * ONE path. No duplication.
   */
  private handleLeave(): void {
    const liveTab = contentState.findItem('live', this.entityId);
    if (liveTab) {
      ContentService.close(liveTab.id);
    } else {
      // Tab not found in contentState (edge case: direct join without tab).
      // Call willClose directly so cleanup always happens.
      this.willClose().catch(err => console.error('LiveWidget: willClose failed:', err));
    }
  }

  /**
   * ContentLifecycle hook — called BEFORE tab is removed.
   * This is THE single cleanup path for leaving a call.
   * Like iOS viewWillDisappear — save state, disconnect, notify server.
   */
  async willClose(): Promise<void> {
    if (!this.isJoined || !this.sessionId) return;
    const sessionId = this.sessionId;

    // 1. Disconnect audio/video immediately
    if (this.audioClient) {
      this.audioClient.leave();
      this.audioClient = null;
    }

    // 2. Reset UI state
    LiveCallTracker.leave(this.entityId);
    this.isJoined = false;
    this.sessionId = null;
    this.participants = [];
    this.isPreview = false;
    this._transcript = [];
    this._transcriptOpen = false;
    this.cleanup();
    this.requestUpdate();

    // 3. Notify server — await to ensure cleanup completes before tab removal
    try {
      await Commands.execute<LiveLeaveParams, LiveLeaveResult>(COMMANDS.COLLABORATION_LIVE_LEAVE, {
        sessionId
      });
    } catch (err) {
      console.error('LiveWidget: leave notify failed:', err);
    }
  }

  // ========================================
  // Event Subscriptions
  // ========================================

  private subscribeToEvents(): void {
    if (!this.sessionId) return;

    // NOTE: Participant join/leave is handled ONLY by LiveKit SDK events
    // (onParticipantJoined/onParticipantLeft callbacks in AudioStreamClient).
    // Server-side `live:joined`/`live:left` events are NOT subscribed here
    // because they race with LiveKit SDK events and caused duplicate/phantom
    // participants (the grid would fluctuate: 16→12→4→8).
    // LiveKit SDK is the ground truth for who is actually connected.

    // NOTE: Speaking state was previously tracked via `live:speaking` server events
    // and timer-based `voice:ai:speech` events. Both are now replaced by LiveKit's
    // ActiveSpeakersChanged which tracks ACTUAL audio levels at the browser.
    // See onActiveSpeakersChanged callback in AudioStreamClient setup above.

    // AI cognitive activity — surface thinking/generating/tool-use on live tiles.
    // Same AI_DECISION_EVENTS the chat widget uses; no new events needed.
    this.unsubscribers.push(
      Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: { personaId: string }) => {
        this._setParticipantActivity(data.personaId, 'thinking');
      }),
      Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: { personaId: string }) => {
        this._setParticipantActivity(data.personaId, 'generating');
      }),
      Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: { personaId: string }) => {
        this._setParticipantActivity(data.personaId, null);
      }),
      Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: { personaId: string }) => {
        this._setParticipantActivity(data.personaId, null);
      }),
      Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: { personaId: string }) => {
        this._setParticipantActivity(data.personaId, null);
      }),
    );

    // AI speech events — used ONLY for captions (text content + duration).
    // Speaking state (green highlight) is now driven by LiveKit ActiveSpeakersChanged
    // which tracks actual audio levels, not timers.
    this.unsubscribers.push(
      Events.subscribe('voice:ai:speech', (data: {
        sessionId: string;
        speakerId: string;
        speakerName: string;
        text: string;
        audioDurationMs?: number;
        timestamp: number;
      }) => {
        if (data.sessionId !== this.sessionId) return;

        // Always log to transcript
        this._appendTranscript(data.speakerName, data.text);

        if (!this.captionsEnabled) return;

        const durationMs = data.audioDurationMs || DEFAULT_AI_SPEECH_DURATION_MS;
        const captions = this._captionsRef.value ?? null;
        if (captions) {
          if (captions.hasCaption(data.speakerName)) {
            captions.extendCaption(data.speakerName, durationMs + CAPTION_EXTEND_BUFFER_MS);
          } else {
            captions.setCaption(data.speakerName, data.text, durationMs + CAPTION_EXTEND_BUFFER_MS);
          }
        }
      })
    );
  }

  // ========================================
  // AI Activity State
  // ========================================

  /** Update a participant's cognitive activity state (immutable for Lit reactivity) */
  private _setParticipantActivity(personaId: string, state: ActivityState): void {
    const idx = this.participants.findIndex(p => p.userId === personaId);
    if (idx === -1) return;
    if (this.participants[idx].activityState === state) return; // no-op
    this.participants = this.participants.map((p, i) =>
      i === idx ? { ...p, activityState: state } : p
    );
  }

  // ========================================
  // Audio State Management
  // ========================================

  private async applyMicState(): Promise<void> {
    if (!this.audioClient?.isConnected) return;

    const wantMic = this._effectiveMic;
    this._lastAppliedMic = wantMic;
    console.log(`LiveWidget: applyMicState(effective=${wantMic}, intent=${this._micIntent}, tab=${this._tabActive}, visible=${this._widgetVisible})`);

    if (wantMic) {
      try {
        await this.audioClient.startMicrophone();
      } catch (error) {
        console.error('LiveWidget: Failed to start mic:', error);
        // Don't reset _micIntent — that's the user's preference.
        // Mark permission as not granted so button shows amber.
        this.micPermissionGranted = false;
      }
    } else {
      this.audioClient.stopMicrophone();
    }
  }

  private async toggleMic(): Promise<void> {
    if (!this.micPermissionGranted) {
      // Permission not yet granted in this tab — clicking means "activate".
      // Browsers require getUserMedia from a user gesture (click). We're in one now.
      // Don't flip intent — user already wants mic ON, they're just activating it.
      if (!this.audioClient?.isConnected) {
        console.warn('LiveWidget: Cannot activate mic — not connected');
        return;
      }
      try {
        await this.audioClient.startMicrophone();
        this.micPermissionGranted = true;
        this._lastAppliedMic = true; // Prevent _syncMediaState from re-calling startMicrophone
        console.log('LiveWidget: Mic activated (first click in this tab)');
      } catch (error) {
        console.warn('LiveWidget: Mic activation failed:', error);
        this.micPermissionGranted = false;
      }
      return;
    }

    // Permission already granted — normal toggle
    this._micIntent = !this._micIntent;
    // _syncMediaState via updated() applies the new effective state to LiveKit
    await this.saveCallState();
  }

  private applySpeakerState(): void {
    if (!this.audioClient) return;
    this.audioClient.setSpeakerMuted(!this._effectiveSpeaker);
    this.audioClient.setSpeakerVolume(this.speakerVolume);
  }

  private async toggleSpeaker(): Promise<void> {
    this._speakerIntent = !this._speakerIntent;
    // _syncMediaState via updated() applies the new effective state
    await this.saveCallState();
  }

  private async toggleCamera(): Promise<void> {
    if (!this.audioClient) {
      console.error('LiveWidget: No audio client for camera toggle');
      return;
    }

    if (!this.cameraPermissionGranted) {
      // Permission not yet granted in this tab — clicking means "activate camera".
      // We're in a user gesture context. Enable camera via LiveKit (triggers getUserMedia).
      try {
        const videoElement = await this.audioClient.setCameraEnabled(true);
        this.cameraEnabled = true;
        this.cameraPermissionGranted = true;
        if (videoElement) {
          const myUserId = this.currentUser?.id || '';
          this._remoteVideoElements.set(myUserId, videoElement);
          this.activeVideoUsers = new Set([...this.activeVideoUsers, myUserId]);
        }
        console.log('LiveWidget: Camera activated (first click in this tab)');
      } catch (error) {
        console.error('LiveWidget: Camera activation failed:', error);
        this.cameraEnabled = false;
        this.cameraPermissionGranted = false;
      }
      return;
    }

    // Permission already granted — normal toggle
    this.cameraEnabled = !this.cameraEnabled;

    try {
      const videoElement = await this.audioClient.setCameraEnabled(this.cameraEnabled);

      if (this.cameraEnabled && videoElement) {
        const myUserId = this.currentUser?.id || '';
        this._remoteVideoElements.set(myUserId, videoElement);
        this.activeVideoUsers = new Set([...this.activeVideoUsers, myUserId]);
        console.log('LiveWidget: Local camera preview enabled');
      } else if (!this.cameraEnabled) {
        const myUserId = this.currentUser?.id || '';
        this._remoteVideoElements.delete(myUserId);
        const newSet = new Set(this.activeVideoUsers);
        newSet.delete(myUserId);
        this.activeVideoUsers = newSet;
        console.log('LiveWidget: Local camera preview disabled');
      }
    } catch (error) {
      console.error('LiveWidget: Failed to toggle camera:', error);
      this.cameraEnabled = false;
      this.cameraPermissionGranted = false;
    }
  }

  private async toggleScreenShare(): Promise<void> {
    this.screenShareEnabled = !this.screenShareEnabled;

    if (this.screenShareEnabled) {
      try {
        const stream = await navigator.mediaDevices.getDisplayMedia({ video: true });
        stream.getVideoTracks()[0].onended = () => {
          this.screenShareEnabled = false;
        };
      } catch (error) {
        console.error('LiveWidget: Failed to share screen:', error);
        this.screenShareEnabled = false;
      }
    }
  }

  private async toggleCaptions(): Promise<void> {
    this.captionsEnabled = !this.captionsEnabled;
    if (!this.captionsEnabled) {
      const captions = this._captionsRef.value ?? null;
      captions?.clearAll();
    }
    await this.saveCallState();
  }

  private toggleTranscript(): void {
    this._transcriptOpen = !this._transcriptOpen;
  }

  /** Append to running transcript log. Deduplicates consecutive identical entries from same speaker. */
  private _appendTranscript(speakerName: string, text: string): void {
    const last = this._transcript[this._transcript.length - 1];
    // Skip if identical to last entry from same speaker (dedup rapid updates)
    if (last && last.speakerName === speakerName && last.text === text) return;
    this._transcript.push({ speakerName, text, timestamp: Date.now() });
    // If transcript panel is open, trigger re-render
    if (this._transcriptOpen) this.requestUpdate();
  }

  // ========================================
  // Speaking State
  // ========================================

  // setSpeaking / setSpeakingWithDuration REMOVED — speaking state is now driven
  // by LiveKit's ActiveSpeakersChanged (actual audio levels at the browser).
  // Timer-based speaking was fundamentally broken: green highlight had no
  // correlation with actual audio due to WebRTC encoding/network latency.
  // Auto-spotlight is also handled in the ActiveSpeakersChanged callback.

  /**
   * Get the video element for a participant (if any).
   * Used in templates to pass video element as a Lit property.
   */
  private _videoFor(userId: string): HTMLVideoElement | null {
    return this._remoteVideoElements.get(userId) ?? null;
  }

  // ========================================
  // Participant Profile
  // ========================================

  private openParticipantProfile(participant: Participant): void {
    const entityId = participant.userId;
    const title = participant.displayName || 'User Profile';

    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    ContentService.open('profile', entityId, {
      title,
      metadata: { user: { id: entityId, displayName: title } }
    });
  }

  // ========================================
  // Event Handlers from Sub-Components
  // ========================================

  private _onParticipantClick(e: CustomEvent): void {
    const userId = e.detail.userId;
    if (this.spotlightUserId === userId) {
      // Already spotlighted — exit back to grid
      this.spotlightUserId = null;
      this._spotlightPinned = false;
    } else {
      // Temporary spotlight — auto-speaker can still override
      this.spotlightUserId = userId;
      // Do NOT set _spotlightPinned — only the pin icon does that
    }
  }

  private _onPinParticipant(e: CustomEvent): void {
    const userId = e.detail.userId;
    if (this.spotlightUserId === userId && this._spotlightPinned) {
      // Already pinned on this user — unpin
      this.spotlightUserId = null;
      this._spotlightPinned = false;
    } else {
      // Pin to this user
      this.spotlightUserId = userId;
      this._spotlightPinned = true;
    }
  }

  private _onExitSpotlight(): void {
    this.spotlightUserId = null;
    this._spotlightPinned = false;
  }

  private _onTileResized(e: CustomEvent): void {
    const { userId, width, height } = e.detail;
    this._tileResolutions.set(userId, { width, height });

    // Debounce 1s — batch all tile resize events into a single data channel publish
    if (this._tileResDebounce) clearTimeout(this._tileResDebounce);
    this._tileResDebounce = setTimeout(() => {
      if (this.audioClient && this._tileResolutions.size > 0) {
        this.audioClient.sendTileResolutions(this._tileResolutions);
      }
    }, TILE_RESOLUTION_DEBOUNCE_MS);
  }

  // ========================================
  // Grid Helpers
  // ========================================

  private _gridDataCount(): string {
    const count = this.participants.length;
    if (count <= 25) return count.toString();
    return 'many';
  }

  // ========================================
  // Cleanup
  // ========================================

  private cleanup(): void {
    if (this.audioClient) {
      this.audioClient.leave();
      this.audioClient = null;
    }

    // NOTE: Do NOT reset _stateLoaded here — user context persists across calls.
    // Only call-specific state (mic applied state, video elements) needs resetting.

    // Reset applied-state tracking so next join gets a clean sync
    this._lastAppliedMic = null;
    this._lastAppliedSpeaker = null;
    this._applyingMicState = false;

    this._remoteVideoElements.clear();
    this.activeVideoUsers = new Set();
    this._tileResolutions.clear();
    if (this._tileResDebounce) {
      clearTimeout(this._tileResDebounce);
      this._tileResDebounce = null;
    }
    if (this._spotlightHoldTimer) {
      clearTimeout(this._spotlightHoldTimer);
      this._spotlightHoldTimer = null;
    }

    this.unsubscribers.forEach(unsub => unsub());
    this.unsubscribers = [];

    if (this.visibilityObserver) {
      this.visibilityObserver.disconnect();
      this.visibilityObserver = null;
    }

    if (this.previewStream) {
      this.previewStream.getTracks().forEach(track => track.stop());
      this.previewStream = null;
    }

    if (this.localStream) {
      this.localStream.getTracks().forEach(track => track.stop());
      this.localStream = null;
    }

    if (this.audioContext) {
      this.audioContext.close();
      this.audioContext = null;
    }
  }

  // ========================================
  // Rendering (Orchestrator)
  // ========================================

  protected override render(): TemplateResult {
    if (this.isJoined && this._stateLoaded) {
      const presenter = this.spotlightUserId
        ? this.participants.find(p => p.userId === this.spotlightUserId)
        : this.participants.find(p => p.screenShareEnabled);

      // Transcript panel rendered at top level — persists across grid↔spotlight switches.
      // Without this, switching layouts destroys/recreates the panel (re-triggers animation, loses scroll).
      const view = presenter
        ? this._renderSpotlightView(presenter)
        : this._renderGridView();

      return html`${view}${this._renderTranscriptPanel()}`;
    }

    if (this.isJoined && !this._stateLoaded) {
      // Joined but state still loading — show connecting state, not stale defaults
      return html`
        <div class="live-container">
          <div class="join-prompt">
            <div class="empty-state">
              <p>Connecting...</p>
            </div>
          </div>
        </div>
      `;
    }

    // Not joined
    return html`
      <div class="live-container">
        <div class="join-prompt">
          <div class="empty-state">
            <p>Select a conversation to start a call</p>
          </div>
        </div>
      </div>
    `;
  }

  private _renderGridView(): TemplateResult {
    return html`
      <div class="live-container"
        @toggle-mic=${() => this.toggleMic()}
        @toggle-speaker=${() => this.toggleSpeaker()}
        @toggle-camera=${() => this.toggleCamera()}
        @toggle-screenshare=${() => this.toggleScreenShare()}
        @toggle-captions=${() => this.toggleCaptions()}
        @toggle-transcript=${() => this.toggleTranscript()}
        @leave=${() => this.handleLeave()}
        @participant-click=${(e: CustomEvent) => this._onParticipantClick(e)}
        @pin-participant=${(e: CustomEvent) => this._onPinParticipant(e)}
        @exit-spotlight=${() => this._onExitSpotlight()}
        @tile-resized=${(e: CustomEvent) => this._onTileResized(e)}
      >
        <div class="content-area">
          <div class="participant-grid" data-count="${this._gridDataCount()}">
            ${this.participants.length === 0
              ? html`<div class="empty-state">Waiting for others to join...</div>`
              : repeat(this.participants, p => p.userId, (p, i) => html`
                  <live-participant-tile
                    data-user-id="${p.userId}"
                    data-color="${(i % 7) + 1}"
                    .userId=${p.userId}
                    .displayName=${p.displayName}
                    .isSpeaking=${p.isSpeaking}
                    .activityState=${p.activityState}
                    .videoElement=${this._videoFor(p.userId)}
                    .isMuted=${!p.micEnabled}
                    .isPinned=${this.spotlightUserId === p.userId && this._spotlightPinned}
                  ></live-participant-tile>
                `)
            }
          </div>
          <live-captions ${ref(this._captionsRef)} .visible=${this.captionsEnabled}></live-captions>
        </div>
        <live-controls ${ref(this._controlsRef)}
          .micEnabled=${this._micIntent && this.micPermissionGranted}
          .micPermissionNeeded=${this._micIntent && !this.micPermissionGranted}
          .speakerEnabled=${this._speakerIntent}
          .cameraEnabled=${this.cameraEnabled && this.cameraPermissionGranted}
          .cameraPermissionNeeded=${this.cameraEnabled && !this.cameraPermissionGranted}
          .screenShareEnabled=${this.screenShareEnabled}
          .captionsEnabled=${this.captionsEnabled}
          .transcriptOpen=${this._transcriptOpen}
          .transcriptCount=${this._transcript.length}
        ></live-controls>
      </div>
    `;
  }

  private _renderSpotlightView(presenter: Participant): TemplateResult {
    const otherParticipants = this.participants.filter(p => p.userId !== presenter.userId);

    return html`
      <div class="live-container spotlight-mode"
        @toggle-mic=${() => this.toggleMic()}
        @toggle-speaker=${() => this.toggleSpeaker()}
        @toggle-camera=${() => this.toggleCamera()}
        @toggle-screenshare=${() => this.toggleScreenShare()}
        @toggle-captions=${() => this.toggleCaptions()}
        @toggle-transcript=${() => this.toggleTranscript()}
        @leave=${() => this.handleLeave()}
        @participant-click=${(e: CustomEvent) => this._onParticipantClick(e)}
        @pin-participant=${(e: CustomEvent) => this._onPinParticipant(e)}
        @exit-spotlight=${() => this._onExitSpotlight()}
        @tile-resized=${(e: CustomEvent) => this._onTileResized(e)}
      >
        <!-- Main presenter area with caption overlay -->
        <div class="content-area">
          <div class="spotlight-main" @click=${() => { this.spotlightUserId = null; this._spotlightPinned = false; }}>
            <live-participant-tile
              class="spotlight-presenter"
              data-user-id="${presenter.userId}"
              .userId=${presenter.userId}
              .displayName=${presenter.displayName}
              .isSpeaking=${presenter.isSpeaking}
              .activityState=${presenter.activityState}
              .videoElement=${this._videoFor(presenter.userId)}
              .isPresenter=${true}
              .isSpotlighted=${!!this.spotlightUserId}
              .isScreenSharing=${presenter.screenShareEnabled}
              .isMuted=${!presenter.micEnabled}
              .isPinned=${this.spotlightUserId === presenter.userId && this._spotlightPinned}
              @click=${(e: Event) => e.stopPropagation()}
            ></live-participant-tile>
          </div>
          <live-captions ${ref(this._captionsRef)} .visible=${this.captionsEnabled}></live-captions>
        </div>

        <!-- Other participants in strip -->
        ${otherParticipants.length > 0 ? html`
          <div class="participant-strip">
            ${repeat(otherParticipants, p => p.userId, (p, i) => html`
              <div class="strip-tile">
                <live-participant-tile
                  data-user-id="${p.userId}"
                  data-color="${(i % 7) + 1}"
                  .userId=${p.userId}
                  .displayName=${p.displayName}
                  .isSpeaking=${p.isSpeaking}
                  .activityState=${p.activityState}
                  .videoElement=${this._videoFor(p.userId)}
                  .isMuted=${!p.micEnabled}
                  .isPinned=${this.spotlightUserId === p.userId && this._spotlightPinned}
                ></live-participant-tile>
              </div>
            `)}
          </div>
        ` : ''}

        <live-controls ${ref(this._controlsRef)}
          .micEnabled=${this._micIntent && this.micPermissionGranted}
          .micPermissionNeeded=${this._micIntent && !this.micPermissionGranted}
          .speakerEnabled=${this._speakerIntent}
          .cameraEnabled=${this.cameraEnabled && this.cameraPermissionGranted}
          .cameraPermissionNeeded=${this.cameraEnabled && !this.cameraPermissionGranted}
          .screenShareEnabled=${this.screenShareEnabled}
          .captionsEnabled=${this.captionsEnabled}
          .transcriptOpen=${this._transcriptOpen}
          .transcriptCount=${this._transcript.length}
        ></live-controls>
      </div>
    `;
  }

  private _renderTranscriptPanel(): TemplateResult {
    if (!this._transcriptOpen) return html``;

    return html`
      <div class="transcript-panel">
        <div class="transcript-header">
          <span class="transcript-title">Transcript</span>
          <span class="transcript-count">${this._transcript.length} entries</span>
          <button class="transcript-close" @click=${() => this.toggleTranscript()} title="Close transcript">&times;</button>
        </div>
        <div class="transcript-body" ${ref(this._transcriptBodyRef)}>
          ${this._transcript.length === 0
            ? html`<div class="transcript-empty">No transcriptions yet. Speak or wait for others to speak.</div>`
            : this._transcript.map(entry => html`
              <div class="transcript-entry">
                <span class="transcript-time">${this._formatTime(entry.timestamp)}</span>
                <span class="transcript-speaker">${entry.speakerName}</span>
                <span class="transcript-text">${entry.text}</span>
              </div>
            `)
          }
        </div>
      </div>
    `;
  }

  private _formatTime(timestamp: number): string {
    const d = new Date(timestamp);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  private async loadExistingParticipants(): Promise<void> {
    if (!this.entityId) return;

    try {
      const result = await DataList.execute<CallEntity>({
        collection: 'calls',
        filter: { roomId: this.entityId, status: 'active' },
        limit: 1,
        dbHandle: 'default'
      });

      if (result.success && result.items?.length > 0) {
        const call = result.items[0];
        this.participants = (call.participants || []).map((p: CallParticipant) => ({
          userId: p.userId,
          displayName: p.displayName || 'Unknown',
          micEnabled: p.micEnabled ?? true,
          cameraEnabled: p.cameraEnabled ?? false,
          screenShareEnabled: p.screenShareEnabled ?? false,
          isSpeaking: false,
          activityState: null,
        }));
        console.log('LiveWidget: Found', this.participants.length, 'existing participants');
      }
    } catch (error) {
      console.warn('LiveWidget: Failed to load existing participants:', error);
    }
  }
}

// Register the custom element
if (typeof customElements !== 'undefined' && !customElements.get('live-widget')) {
  customElements.define('live-widget', LiveWidget);
}
