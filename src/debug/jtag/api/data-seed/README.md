# Data Seeding System - Complete Setup for New Repo Users

## 🚀 Quick Start for New Repo Users

**CRITICAL**: These scripts provide repeatable, reliable setup for the entire JTAG system. Essential for consistent development environment.

### 1. Complete System Setup (Recommended)
```bash
cd src/debug/jtag
npm start                           # Start JTAG system (REQUIRED FIRST)
npx tsx api/data-seed/seed-data.ts  # Complete reset and seed
```

### 2. Clear All Data (Reset System)
```bash
npx tsx api/data-seed/clear-data.ts  # Clear everything
```

### 3. Individual Operations
```bash
# Users only
npx tsx api/data-seed/seedUsers.ts

# Verify what's in the system
./jtag data/list --collection="users"
./jtag data/list --collection="rooms"
./jtag data/list --collection="chat_messages"
```

## 📋 What Gets Created

### 👥 Users (6 total)
- **joel** (user-joel-12345) - Human user, project owner, authenticated
- **Claude Code** (claude-code-agent) - AI code assistant, full system access
- **GeneralAI** (general-ai-persona) - General purpose AI assistant
- **CodeAI** (code-ai-agent) - Code analysis specialist 
- **PlannerAI** (planner-ai-agent) - Strategic planning assistant
- **Auto Route** (auto-route-agent) - Smart task routing system

### 🏠 Rooms (Based on existing JSON data)
- **General** - Main discussion room (all 6 users)
- **Academy** - Educational discussions (joel + AI assistants)
- **Support** - Technical support (if configured in JSON)
- **AI Training** - AI development discussions (if configured in JSON)

### 💬 Initial Messages
- Welcome messages in each room
- Claude Code introduction
- System ready notifications

## 🔧 Architecture Benefits

### ✅ Repeatable & Reliable
- **Same setup every time** - No manual steps, no variations
- **Crash and burn error handling** - Clear failures, no hidden issues
- **Verification built-in** - Confirms all data created correctly

### ✅ Extensible Design
- **Add new users** - Extend UserDataSeed.generateSeedUsers()
- **Add new rooms** - Update JSON files or RoomDataSeed
- **Add new message types** - Extend message generation logic

### ✅ API-Driven Architecture
- **No manual filesystem calls** - Uses JTAG data/create commands
- **Proper daemon architecture** - Follows system design patterns
- **Centralized data logic** - All seeding logic in api/data-seed/

## 📁 File Structure

```
api/data-seed/
├── DataSeeder.ts          # Main orchestration - complete reset/seed
├── UserDataSeed.ts        # User generation with strict typing
├── RoomDataSeed.ts        # Room generation from JSON data  
├── seedUsers.ts           # Individual user seeding
├── seed-data.ts           # Complete system setup (MAIN ENTRY)
├── clear-data.ts          # Complete data clearing
└── README.md              # This documentation
```

## 🎯 For New Repo Users

### First Time Setup
1. Clone repository
2. `cd src/debug/jtag`
3. `npm install`
4. `npm start` (starts system, opens browser)
5. `npx tsx api/data-seed/seed-data.ts` (creates all data)
6. System ready! 🎉

### Daily Development
- **Reset when needed**: `npx tsx api/data-seed/seed-data.ts`
- **Clear for testing**: `npx tsx api/data-seed/clear-data.ts`
- **Check system state**: `./jtag data/list --collection="users"`

### When Things Go Wrong
- **Data corruption**: `npx tsx api/data-seed/seed-data.ts` (complete reset)
- **Partial failures**: Check console output - crash and burn error handling shows exact issue
- **System state unclear**: Use verification commands to see what exists

## 🔍 Integration with Existing Data

### JSON Configuration Files
The system automatically loads from existing configuration:
- `data/initial-chat-rooms.json` - Room definitions
- `data/chat-rooms-initial.json` - Extended room data

### Backward Compatibility
- **Existing tests still work** - Data structure matches expectations
- **Widget integration ready** - User list widget can consume this data
- **Chat system compatible** - Messages work with existing chat architecture

## 🚨 Critical for New Repo Users

### Why This Matters
- **Eliminates setup friction** - New developers get working system immediately
- **Prevents inconsistent states** - Everyone has same data setup
- **Enables reliable testing** - Known good state for development
- **Documentation through code** - Setup process is self-documenting

### Success Indicators
After running `npx tsx api/data-seed/seed-data.ts`, you should see:
```
🎉 COMPLETE! System ready with fresh data for new repo users
👥 Users: joel + 5 AI agents (Claude Code, GeneralAI, CodeAI, PlannerAI, Auto Route)  
🏠 Rooms: general (6 members), academy (3 members)
💬 Messages: Welcome messages in both rooms
✅ All data verified and ready for development
```

This output confirms the system is ready for development work.

## 🔄 Maintenance

### Adding New Default Users
1. Edit `UserDataSeed.generateSeedUsers()`
2. Add new user creation logic
3. Test with `npx tsx api/data-seed/seed-data.ts`

### Adding New Default Rooms
1. Update `data/initial-chat-rooms.json` or `data/chat-rooms-initial.json`
2. Test with `npx tsx api/data-seed/seed-data.ts`

### Modifying Default Messages
1. Edit `RoomDataSeed.generateSeedMessages()`
2. Test with `npx tsx api/data-seed/seed-data.ts`

## 💡 Development Philosophy

**"Fallbacks are illegal and bad. Crash and burn is better."**
- No silent failures or partial success states
- Clear error messages when things go wrong  
- Failed seeding means system stops - fix the issue, don't work around it
- This prevents days of debugging mysterious issues from partial setup

**"Typing like Rust - strict, explicit, and predictable"**
- Branded types (UserId, RoomId) prevent mix-ups
- Compile-time safety where possible
- Explicit error handling, no hidden failures

This system ensures every new repo user has the same reliable, well-typed foundation for development.