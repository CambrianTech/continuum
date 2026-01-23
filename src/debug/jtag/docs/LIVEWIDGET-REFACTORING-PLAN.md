# LiveWidget Refactoring Plan

## Current State
- **File**: `widgets/live/LiveWidget.ts`
- **Size**: 1026 lines (205% over 500 line limit)
- **Methods/Properties**: 56
- **Problem**: Violates single responsibility principle - handles UI, state, media, networking, and rendering

## Target Architecture

Split into 4 focused modules, each < 500 lines:

```
widgets/live/
├── LiveWidget.ts                    (~250 lines) - Main component, coordinates modules
├── LiveCallState.ts                 (~200 lines) - State management & persistence
├── LiveMediaManager.ts              (~300 lines) - Media streams, audio/video/screen
└── LiveParticipantRenderer.ts       (~250 lines) - UI rendering & participant grid
```

---

## Module 1: LiveCallState.ts (~200 lines)

**Purpose**: Centralized state management for call settings and participant state

**Responsibilities**:
- Manage all @reactive properties
- Load/save state to UserStateEntity
- Speaking state tracking (timeouts per user)
- Caption state management
- Provide typed state accessors

**Exports**:
```typescript
export class LiveCallState {
  // Session state
  sessionId: string | null = null;
  participants: Participant[] = [];
  isJoined: boolean = false;
  isPreview: boolean = false;
  previewStream: MediaStream | null = null;

  // Local user state
  micEnabled: boolean = true;
  speakerEnabled: boolean = true;
  speakerVolume: number = 1.0;
  micLevel: number = 0;
  cameraEnabled: boolean = false;
  screenShareEnabled: boolean = false;
  micPermissionGranted: boolean = false;
  captionsEnabled: boolean = true;
  currentCaption: { speakerName: string; text: string; timestamp: number } | null = null;

  // Entity association
  entityId: string = '';

  // Speaking state
  private speakingTimeouts: Map<UUID, ReturnType<typeof setTimeout>> = new Map();

  // Methods
  async loadCallState(userStateId: UUID): Promise<void>
  async saveCallState(userStateId: UUID): Promise<void>
  setSpeaking(userId: UUID, isSpeaking: boolean): void
  setCaption(speakerName: string, text: string): void
  clearSpeaking(userId: UUID): void
  addParticipant(participant: Participant): void
  removeParticipant(userId: UUID): void
  updateParticipant(userId: UUID, updates: Partial<Participant>): void
  findParticipant(userId: UUID): Participant | undefined
}
```

**Code Moved From LiveWidget.ts**:
- Lines 42-76: All @reactive properties and private state
- Lines 101-186: loadCallState(), saveCallState()
- Lines 629-656: setCaption()
- Lines 657-682: setSpeaking()
- Caption fade timeout management
- Speaking timeouts map

**Benefits**:
- Single source of truth for all call state
- Easy to test state transitions
- Persistence logic isolated
- Can be reused by other widgets (future: CallControlPanel, etc.)

---

## Module 2: LiveMediaManager.ts (~300 lines)

**Purpose**: Handle all media stream operations (mic, camera, screen, audio routing)

**Responsibilities**:
- Capture getUserMedia (mic/camera/screen)
- Manage AudioContext and audio routing
- Handle AudioStreamClient (WebSocket to Rust server)
- Monitor mic levels
- Manage media permissions
- Stream lifecycle (start/stop/cleanup)

**Exports**:
```typescript
export class LiveMediaManager {
  private localStream: MediaStream | null = null;
  private audioContext: AudioContext | null = null;
  private audioClient: AudioStreamClient | null = null;
  private micLevelInterval: ReturnType<typeof setInterval> | null = null;

  // Initialization
  async requestMicrophonePermission(): Promise<boolean>
  async startPreview(): Promise<MediaStream>
  async startLocalStreams(micEnabled: boolean, cameraEnabled: boolean): Promise<void>

  // Media controls
  async toggleMicrophone(currentState: boolean): Promise<boolean>
  async toggleCamera(currentState: boolean): Promise<boolean>
  async toggleScreenShare(currentState: boolean): Promise<boolean>

  // Audio
  async startAudioStreaming(sessionId: string, userId: UUID): Promise<void>
  async stopAudioStreaming(): Promise<void>
  setMicrophoneLevel(callback: (level: number) => void): void
  stopMicrophoneLevelMonitoring(): void

  // Volume
  setSpeakerVolume(volume: number): void
  setSpeakerEnabled(enabled: boolean): void

  // Cleanup
  cleanup(): void
}
```

**Code Moved From LiveWidget.ts**:
- Lines 63-67: localStream, audioContext, audioClient
- Lines 230-242: startPreview()
- Lines 508-535: toggleMic()
- Lines 536-551: toggleSpeaker()
- Lines 552-559: setSpeakerVolume()
- Lines 560-590: toggleCamera()
- Lines 591-618: toggleScreenShare()
- Mic level monitoring logic (AudioContext analyser setup)
- AudioStreamClient integration

**Benefits**:
- All media API interactions in one place
- Easy to test media functionality
- Can mock for unit tests
- Isolates browser API dependencies

---

## Module 3: LiveParticipantRenderer.ts (~250 lines)

**Purpose**: Render participant grid, spotlight view, and all UI icons

**Responsibilities**:
- Render participant tiles (grid layout)
- Render spotlight view (screen share / presenter mode)
- Render all control icons (mic, camera, speaker, etc.)
- Handle participant click events
- Grid layout calculation (data-count attribute)

**Exports**:
```typescript
export class LiveParticipantRenderer {
  // Grid rendering
  renderParticipantGrid(participants: Participant[]): TemplateResult
  renderParticipant(participant: Participant, onClick: (p: Participant) => void): TemplateResult
  getGridDataCount(participantCount: number): string

  // Spotlight mode
  renderSpotlightView(presenter: Participant, otherParticipants: Participant[]): TemplateResult

  // Icons
  renderMicOnIcon(): TemplateResult
  renderMicOffIcon(): TemplateResult
  renderCameraOnIcon(): TemplateResult
  renderCameraOffIcon(): TemplateResult
  renderSpeakerOnIcon(): TemplateResult
  renderSpeakerOffIcon(): TemplateResult
  renderScreenShareIcon(): TemplateResult
  renderLeaveIcon(): TemplateResult
  renderCaptionsIcon(): TemplateResult
  renderMutedIndicator(): TemplateResult
}
```

**Code Moved From LiveWidget.ts**:
- Lines 804-832: renderParticipant()
- Lines 833-947: All renderXxxIcon() methods
- Lines 948-1026: renderSpotlightView()
- Lines 710-715: getGridDataCount()

**Benefits**:
- Pure rendering logic (no side effects)
- Easy to add new icons/layouts
- Can snapshot test rendering
- Reusable across widgets

---

## Module 4: LiveWidget.ts (Main Component) (~250 lines)

**Purpose**: Coordinate modules, handle lifecycle, route events

**Responsibilities**:
- LitElement lifecycle (connectedCallback, disconnectedCallback)
- Compose modules (state, media, renderer)
- Handle user interactions (button clicks)
- Subscribe to Events (participant_joined, transcription, etc.)
- Route commands (join, leave)
- Main render() method (calls renderer)

**Keeps**:
```typescript
export class LiveWidget extends ReactiveWidget {
  private state: LiveCallState;
  private media: LiveMediaManager;
  private renderer: LiveParticipantRenderer;
  private unsubscribers: Array<() => void> = [];

  constructor() {
    super();
    this.state = new LiveCallState();
    this.media = new LiveMediaManager();
    this.renderer = new LiveParticipantRenderer();
  }

  // Lifecycle
  override connectedCallback(): void
  override disconnectedCallback(): void
  private cleanup(): void

  // Session management
  private async handleJoin(): Promise<void>
  private async handleLeave(): Promise<void>
  private async loadExistingParticipants(): Promise<void>

  // Event subscriptions
  private subscribeToEvents(): void
  private handleParticipantJoined(event): void
  private handleParticipantLeft(event): void
  private handleTranscription(result: TranscriptionResult): void

  // UI event handlers (delegate to modules)
  private async toggleMic(): Promise<void>
  private async toggleSpeaker(): Promise<void>
  private toggleCaptions(): void
  private openParticipantProfile(participant: Participant): void

  // Main render (composes renderer output)
  protected override render(): TemplateResult
}
```

**Code to Keep in LiveWidget.ts**:
- Lines 40-41: Class declaration
- Lines 79-82: Styles
- Lines 84-96: connectedCallback()
- Lines 187-229: disconnectedCallback(), cleanup()
- Lines 243-273: loadExistingParticipants()
- Lines 274-285: togglePreviewMic()
- Lines 286-294: handleCancel()
- Lines 295-448: handleJoin()
- Lines 449-476: handleLeave()
- Lines 477-507: subscribeToEvents()
- Lines 619-628: toggleCaptions()
- Lines 683-709: openParticipantProfile()
- Lines 716-803: Main render()

**Benefits**:
- Thin coordinator, delegates to focused modules
- Easy to follow control flow
- Clear dependency injection (modules in constructor)
- Can swap module implementations for testing

---

## Implementation Steps

### Phase 1: Extract LiveCallState (~1 hour)
1. Create `LiveCallState.ts`
2. Move all @reactive properties and state management
3. Add load/save methods with proper error handling
4. Update LiveWidget to use `this.state.xxx` instead of `this.xxx`
5. Test: Verify state persistence works (mute button survives refresh)

### Phase 2: Extract LiveMediaManager (~2 hours)
1. Create `LiveMediaManager.ts`
2. Move all media stream logic and AudioStreamClient
3. Add proper cleanup in `cleanup()`
4. Update LiveWidget to call `this.media.toggleMicrophone()` etc.
5. Test: Verify mic/camera/screen share still work

### Phase 3: Extract LiveParticipantRenderer (~1 hour)
1. Create `LiveParticipantRenderer.ts`
2. Move all rendering methods (icons, participant tiles, spotlight)
3. Make methods pure (no side effects, return TemplateResult)
4. Update LiveWidget render() to call `this.renderer.renderParticipant()`
5. Test: Visual regression test with screenshot

### Phase 4: Slim Down LiveWidget (~30 minutes)
1. Remove all moved code
2. Update imports
3. Add module composition in constructor
4. Verify lifecycle hooks properly cleanup modules
5. Test: Full integration test (join call, toggle controls, leave)

### Phase 5: Add Tests (~1 hour)
1. Unit tests for LiveCallState (state transitions)
2. Unit tests for LiveMediaManager (mock getUserMedia)
3. Snapshot tests for LiveParticipantRenderer
4. Integration test for LiveWidget (full flow)

---

## Validation Criteria

### Must Pass:
1. ✅ All files < 500 lines
2. ✅ LiveWidget compiles without errors
3. ✅ Can join call and see participants
4. ✅ Mic/camera/screen share controls work
5. ✅ Speaking indicators show when users talk
6. ✅ Captions display transcriptions
7. ✅ Mute state persists across page refresh
8. ✅ No regressions in existing functionality

### Bonus (Future):
- Add TypeScript strict mode to all modules
- Add unit tests for each module
- Mock AudioStreamClient for offline testing
- Snapshot test participant grid layouts

---

## Risk Mitigation

**Risk 1**: Breaking existing functionality
- **Mitigation**: Refactor incrementally, test after each module extraction

**Risk 2**: Reactive properties stop working
- **Mitigation**: Keep @reactive decorators on LiveWidget properties, proxy to state object

**Risk 3**: Circular dependencies between modules
- **Mitigation**: Use dependency injection (pass modules in constructor), avoid imports between extracted modules

**Risk 4**: AudioStreamClient integration breaks
- **Mitigation**: Create interface for AudioStreamClient, mock in tests

---

## Success Metrics

- **Before**: 1 file, 1026 lines, 56 methods
- **After**: 4 files, ~250 lines each, ~14 methods per file
- **Maintainability**: Each module has single responsibility
- **Testability**: Can unit test each module independently
- **Readability**: New developer can understand each module in < 5 minutes

---

## Timeline

- Phase 1 (LiveCallState): 1 hour
- Phase 2 (LiveMediaManager): 2 hours
- Phase 3 (LiveParticipantRenderer): 1 hour
- Phase 4 (Slim LiveWidget): 30 minutes
- Phase 5 (Tests): 1 hour
- **Total**: ~5.5 hours

---

## Files to Create

1. `/widgets/live/LiveCallState.ts` (~200 lines)
2. `/widgets/live/LiveMediaManager.ts` (~300 lines)
3. `/widgets/live/LiveParticipantRenderer.ts` (~250 lines)
4. `/widgets/live/LiveWidget.ts` (refactored to ~250 lines)

## Files to Update

- None (only creates new files and refactors existing)

---

This plan ensures each module has a clear, testable responsibility while maintaining all existing functionality.
