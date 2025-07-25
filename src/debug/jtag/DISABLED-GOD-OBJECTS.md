# Disabled God Objects - Architectural Violations

**Status**: Temporarily disabled to get core modular system running  
**Priority**: HIGH - Must fix and re-enable after core bugs resolved  
**Pattern Violation**: All files violate ~50 line modular command architecture

## 🚫 Disabled Files (Moved to .bak)

### AI Provider Daemon (20k lines)
- `daemons/ai-provider-daemon/shared/AIProviderTypes.ts.bak` (20,074 lines)
  - **Violation**: Massive type collection instead of modular command types
  - **Fix Strategy**: Split into individual command modules following screenshot pattern

### Chat Daemon System (Entire Directory)
- `daemons/chat-daemon.bak/` (entire directory disabled)
  - `shared/AcademyChatInterfaceTypes.ts.bak` (17,520 lines)
  - `shared/ActivePersonaTypes.ts.bak` (27,401 lines) 
  - `shared/PersonaChatIntegrationTypes.ts.bak` (24,254 lines)
  - `shared/AcademyChatRoomTypes.ts.bak` 
  - `shared/MultiContextAwarenessTypes.ts.bak`
  - `shared/UniversalCitizenTypes.ts.bak` (15,813 lines)
  - `shared/ChatDaemon.ts.bak` (18,066 lines)
  - `shared/ChatEvents.ts.bak`
  - **Violation**: Massive daemon instead of modular commands
  - **Fix Strategy**: Break into individual chat commands following modular pattern

### Academy Command God Objects
- `daemons/command-daemon/commands/academy/track-usage/shared/TrackUsageTypes.ts.bak` (18,342 lines)
- `daemons/command-daemon/commands/academy/chat-training/shared/ChatTrainingTypes.ts.bak` (20,206 lines)
- `daemons/command-daemon/commands/academy/interface-training/shared/InterfaceTrainingTypes.ts.bak` (23,092 lines)
- `daemons/command-daemon/commands/academy/multimodal-training/shared/MultimodalTrainingTypes.ts.bak` (23,973 lines)
- `daemons/command-daemon/commands/academy/interface-adapters/shared/InterfaceAdapterTypes.ts.bak` (20,756 lines)
- `daemons/command-daemon/commands/academy/shared/AcademyTypes.ts.bak`
- **Violation**: Individual commands with 20k+ line type files
- **Fix Strategy**: Each should be ~50 line modular command following screenshot template

### Chat Commands (Entire Directory)
- `daemons/command-daemon/commands/chat.bak/` (entire directory disabled)
  - `get-chat-history/shared/GetChatHistoryTypes.ts.bak` (20,371 lines)
  - `room-events/shared/RoomEventTypes.ts.bak` (26,109 lines)  
  - `send-room-event/shared/SendRoomEventTypes.ts.bak` (15,465 lines)
  - **Violation**: Individual chat commands with 15k-26k line type files
  - **Fix Strategy**: Each should be ~50 line modular command

### Transport Layer
- `transports/udp-multicast/UDPMulticastTransport.ts.bak` (29,947 lines)
  - **Violation**: Massive transport implementation instead of following established transport pattern
  - **Fix Strategy**: Follow existing WebSocket transport pattern (~100 lines)
- **TransportFactory.ts**: Disabled UDP multicast transport creation methods
  - **What was disabled**: `createUDPMulticastTransport()` method and all UDP imports
  - **Error handling**: Throws clear error message when UDP transport requested
  - **Status**: Transport factory still works for WebSocket/HTTP transports

### Router God Objects
- `shared/AcademyJTAGRouter.ts.bak` (20,965 lines) **[TO BE DELETED]**
  - **Violation**: Massive router extension trying to handle Academy functionality
  - **What it was doing**:
    - Academy command routing (`/academy/**` paths)
    - Session management routing (`/session/**` paths) 
    - Multimodal training routing (`/multimodal/**` paths)
    - Cross-daemon coordination
    - Session context extraction and management
  - **Why it's wrong**: Router should just route, not manage business logic
  - **Correct approach**: Regular JTAGRouter + individual Academy commands (~50 lines each)
  - **Status**: No imports found - safe to delete entirely

## 📊 Statistics

- **Total Files Disabled**: 20+ files
- **Total Lines Disabled**: ~300,000+ lines
- **Average File Size**: 15,000+ lines  
- **Target Size After Fix**: ~50 lines per module

## 🎯 Re-enabling Strategy

### Phase 1: Fix Core Bugs First
- ✅ Get modular commands working (screenshot, navigate, click, type, etc.)
- 🔄 Fix remaining API issues in file commands
- 🔄 Fix transport factory imports

### Phase 2: Re-enable and Modularize
1. **Start with smallest violations** (15k line files first)
2. **Apply screenshot pattern** to each god object:
   - Split into ~50 line modules
   - Move types to `/shared` within each module
   - Use abstract base classes for shared logic
   - Create thin browser/server overrides
3. **Test each module individually** before moving to next
4. **Update imports** in structure.ts files

### Phase 3: Architecture Validation
- Ensure no file exceeds 100 lines
- Verify import boundaries (no shared→server/browser)
- Confirm ~50 line modular pattern compliance
- Run full system integration tests

## 🚨 Critical Notes

- **DO NOT DELETE** .bak files until replacements are working
- **Follow screenshot template exactly** - it's the proven working pattern
- **Test incrementally** - don't re-enable multiple god objects at once
- **Maintain import boundaries** - shared stays universal, no cross-contamination

## ✅ Success Metrics

Current working modular commands prove the pattern works:
- screenshot (54 lines) ✅
- navigate (54 lines) ✅  
- click (52 lines) ✅
- type (41 lines) ✅
- scroll (~50 lines) ✅
- get-text (~50 lines) ✅
- wait-for-element (~50 lines) ✅

**Goal**: All disabled god objects become similar ~50 line modular commands.