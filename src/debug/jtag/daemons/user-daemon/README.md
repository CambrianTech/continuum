# UserDaemon - Living User Management System

## Overview

UserDaemon manages living user objects (BaseUser hierarchy) for chat participation, AI training, and user lifecycle operations. Unlike UserData (UI display objects), BaseUser objects are **server-only living citizens** that participate in conversations and evolve through interactions.

## Architecture

### Server-Only Domain (BaseUser Hierarchy)
```
BaseUser (abstract)
├── HumanUser extends BaseUser
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser     (external AI: Claude, GPT, etc.)
    └── PersonaUser extends AIUser   (trainable personas with LoRA)
```

### Browser Domain (UserData)
- UI representation objects for rendering
- Managed via existing CommandDaemon.execute('data/list', ...)
- **No change needed** - already working

### Boundary Crossing
- Browser → Server: Commands like `'user/create-persona'`, `'user/train-from-chat'`
- Server → Browser: Events for presence updates, new users joining
- **No static interfaces, no eval() hacks, no dynamic imports**

## Key Responsibilities

### UserDaemonServer (Server-Only)
1. **User Lifecycle Management**
   - Create/destroy HumanUser, PersonaUser, AgentUser instances
   - Manage authentication and session binding
   - Handle user presence and status updates

2. **Chat Integration**
   - Enable BaseUser objects to participate in conversations
   - Route messages to/from PersonaUsers for AI responses
   - Track conversation context for training data

3. **LoRA Training System**
   - PersonaUsers learn from chat interactions
   - Process conversation history into training datasets
   - Manage LoRA adapter creation and updates
   - Handle personality evolution over time

4. **AI Response Generation**
   - PersonaUsers generate contextual responses in chat
   - Maintain conversation memory and personality consistency
   - Integrate with external AI APIs when needed

### Commands (Cross-Boundary Operations)
- `user/create-human` - Create new human user from session
- `user/create-persona` - Create trainable AI persona
- `user/train-persona` - Trigger LoRA training from chat data
- `user/generate-response` - Get AI response from PersonaUser
- `user/update-presence` - Update online/offline status
- `user/authenticate-session` - Validate session → BaseUser mapping

## Implementation Strategy

### Phase 1: Core User Management
- [ ] Implement UserDaemonServer with basic CRUD operations
- [ ] Create BaseUser → UserData serialization bridge
- [ ] Test with existing chat widgets (no UI changes needed)

### Phase 2: Chat Integration
- [ ] Enable PersonaUsers to participate in chat rooms
- [ ] Implement message routing to AI personas
- [ ] Add conversation context tracking

### Phase 3: LoRA Training Infrastructure
- [ ] Build training data collection from chat history
- [ ] Implement LoRA adapter management
- [ ] Create personality evolution system

### Phase 4: Advanced Features
- [ ] Multi-room persona management
- [ ] External AI agent integration
- [ ] Advanced memory and context systems

## Data Flow Example

### User Creation
```
Browser: CommandDaemon.execute('user/create-persona', { name: 'Assistant', personality: 'helpful' })
    ↓
Server: UserDaemonServer.createPersona()
    ↓
Result: New PersonaUser instance created, stored in user registry
    ↓
Browser: Receives success response, updates UI via existing UserData patterns
```

### Chat Participation
```
Human sends message in room
    ↓
Server: UserDaemonServer routes to all PersonaUsers in room
    ↓
PersonaUser generates response using current LoRA adapter
    ↓
Response sent to room, logged for future training
    ↓
Browser: Displays AI response via existing chat widgets
```

## Integration Points

### Existing Systems (No Changes)
- **UserListWidget**: Uses CommandDaemon.execute('data/list') - keep as-is
- **ChatWidget**: Message display - keep using UserData
- **Database**: UserData storage via DataDaemon - no changes

### New Integration
- **Chat Message Routing**: Route to PersonaUsers for AI responses
- **Training Pipeline**: Collect chat data for LoRA training
- **Presence System**: Sync BaseUser status with UserData display

## Benefits

1. **Clean Separation**: UI (UserData) vs Business Logic (BaseUser)
2. **Scalable AI**: PersonaUsers can evolve and learn independently
3. **No Boundary Violations**: Server-only AI processing, browser-only UI
4. **Backwards Compatible**: Existing widgets continue working unchanged
5. **Future-Proof**: Ready for advanced AI features and external integrations

## Next Steps

1. Implement UserDaemonServer basic operations
2. Create user management commands
3. Test with existing chat system
4. Add LoRA training infrastructure incrementally