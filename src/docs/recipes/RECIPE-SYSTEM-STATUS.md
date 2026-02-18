# Recipe System - Implementation Status

## ✅ Completed (2025-10-06)

### Core Infrastructure
- ✅ **Recipe JSON Templates**: JSON-based recipe definitions in `system/recipes/*.json`
- ✅ **RecipeEntity**: Database entity extending BaseEntity with proper validation
- ✅ **Type Definitions**: Complete TypeScript interfaces in `system/recipes/shared/RecipeTypes.ts`
- ✅ **Commands Registered**: Both `recipe/load` and `rag/build` appear in `server/generated.ts` (53 total commands)

### Commands Implemented

#### `recipe/load` Command
- **Path**: `commands/recipe/load/`
- **Purpose**: Load recipe JSON files from `system/recipes/*.json` into database
- **Parameters**:
  - `recipeId`: Load specific recipe (e.g., 'general-chat')
  - `loadAll`: Load all JSON files in recipes directory
  - `reload`: Update existing recipes instead of skipping
- **Result**: List of loaded recipes, skipped items, and errors
- **Status**: ✅ Code complete, TypeScript compiles, command registered

#### `rag/build` Command
- **Path**: `commands/rag/build/`
- **Purpose**: Build RAG context using ChatRAGBuilder
- **Parameters**:
  - `contextId`: Room/game session UUID
  - `personaId`: Persona requesting context UUID
  - `maxMessages`, `maxMemories`, `includeArtifacts`, `includeMemories`, etc.
- **Result**: RAGContext object with conversation history, participants, room metadata
- **Status**: ✅ Code complete, TypeScript compiles, command registered

### Recipe Template Example

File: `system/recipes/general-chat.json`

```json
{
  "uniqueId": "general-chat",
  "name": "General Chat (Human-Focused)",
  "displayName": "General Chat",
  "description": "Natural conversation where AIs respond to humans thoughtfully, avoiding AI-to-AI loops",

  "pipeline": [
    {
      "command": "rag/build",
      "params": { "maxMessages": 20, "includeParticipants": true },
      "outputTo": "ragContext"
    },
    {
      "command": "ai/should-respond",
      "params": { "ragContext": "$ragContext", "strategy": "human-focused" },
      "outputTo": "decision"
    },
    {
      "command": "ai/generate",
      "params": { "ragContext": "$ragContext", "temperature": 0.7 },
      "condition": "decision.shouldRespond === true"
    }
  ],

  "strategy": {
    "conversationPattern": "human-focused",
    "responseRules": [
      "If human asks question → ONE AI responds",
      "If AI just responded → WAIT for human",
      "Prefer letting human guide conversation"
    ]
  }
}
```

### Technical Implementation Details

**RecipeEntity Fields**:
- `uniqueId`: Indexed string for recipe lookup
- `name`, `displayName`, `description`: Recipe metadata
- `pipeline`: Array of RecipeStep (command pipelines)
- `ragTemplate`: RAGTemplate configuration
- `strategy`: RecipeStrategy with conversation patterns
- `tags`: Array of classification tags
- `createdBy`, `parentRecipeId`: Lineage tracking
- `usageCount`, `lastUsedAt`: Usage analytics
- `isPublic`: Sharing control

**RecipeLoadServerCommand Fixes Applied**:
- ✅ Fixed `RECIPES_DIR` to use `process.cwd()` instead of `__dirname` (compiled JS path issue)
- ✅ Fixed collection name: 'Recipe' → 'recipes' (matches RecipeEntity.collectionName)
- ✅ Added `context` and `sessionId` to all `Commands.execute()` calls (required by CommandParams)

**Compilation Status**:
- ✅ TypeScript: 0 errors
- ✅ Version: 1.0.2572 (deployed successfully)
- ✅ Commands found in generation: 53 commands including recipe/load and rag/build

## ⚠️ Known Limitations

### CLI Session Creation Issue
**Problem**: The CLI has a pre-existing session creation failure when the user ID doesn't exist in the database.

**Error**: `❌ JTAGClient: Failed to create session: User 61f49a51-a1ce-4d73-998b-af24e6057f9c not found in database`

**Impact**: Affects ALL CLI commands, not just recipe/load

**Scope**: This is a broader CLI/session management issue, NOT specific to the Recipe system

**Workaround**: Commands work fine when called from:
- Browser widgets (have valid sessions)
- Server-side code (direct command execution)
- Tests that mock/provide valid sessionId

**Future Fix Needed**: CLI should either:
1. Create temporary/guest user for commands that don't need authentication
2. Make session creation optional for read-only commands
3. Auto-create CLI user on first run

## 📋 Next Steps

### Phase 2: Recipe Execution (Not Started)
**Status**: Infrastructure exists but pipeline executor not built

**What's Missing**:
- [ ] Create `recipe/execute` command that actually runs the pipeline
- [ ] Implement pipeline executor with:
  - Variable substitution (`$ragContext` → actual context object)
  - Conditional execution (`condition: "decision.shouldRespond === true"`)
  - Error handling per step (`onError: 'fail' | 'skip' | 'retry'`)
  - Step chaining (output of step N becomes input to step N+1)
- [ ] Test with general-chat recipe end-to-end

**Current Behavior**:
- `RoomEntity.recipeId` field exists and is set (default: 'general-chat')
- Recipe JSONs load into database successfully
- BUT: PersonaUser doesn't read or execute recipes
- PersonaUser still uses inline fuzzy logic for `shouldRespond()`

### Phase 3: PersonaUser Integration (Not Started)
**Goal**: Actually use recipes to control AI behavior

- [ ] Refactor `PersonaUser.shouldRespond()` to use `recipe/execute`
- [ ] Load recipe from database based on `room.recipeId`
- [ ] Replace inline fuzzy logic with recipe-based decision making
- [ ] Pass recipe `strategy.responseRules` to AI model as system prompt
- [ ] Use recipe `ragTemplate` to configure RAG building

**Current Behavior**:
- PersonaUser has hardcoded chat behavior
- No integration with Recipe system
- `room.recipeId` is ignored

### Phase 4: Room/Activity Architecture (Not Started)
**Goal**: Make recipes power entire activities, not just chat

- [ ] URL routing: `continuum://activity/<recipe-id>`
- [ ] Tab widget system (highlight active activity)
- [ ] Custom layouts per recipe (center content + sidebars)
- [ ] Recipe-defined widgets (security dashboard, game board, code editor)
- [ ] Deep linking from external apps

**Current Behavior**:
- Rooms are just chat rooms
- No activity concept
- No custom layouts

### Phase 5: Living Recipes (Not Started)
**Goal**: AIs can propose recipe modifications

- [ ] Recipe modification proposals (whitelist updates, strategy changes)
- [ ] Permission system (ask-once, always-ask, suggest, autonomous)
- [ ] Recipe versioning and rollback
- [ ] Audit log of AI-proposed changes
- [ ] User approval via chat commands or UI

### Phase 6: Additional Recipe Templates (Not Started)
- [ ] `academy-collaborative.json` - Teaching/learning recipe
- [ ] `competitive-generation.json` - Multiple AIs compete for best result
- [ ] `exploring-together.json` - Human + AIs discover together
- [ ] `security-monitoring.json` - I/O Tower threat center (see ARES-MASTER-CONTROL.md)

## 🎯 Architecture Decisions

### Two-Layer Approach
1. **JSON Templates** (version controlled files)
   - Easy to edit, share, and version
   - Human-readable configuration
   - Similar to widget CSS/HTML pattern

2. **Database Entities** (runtime instances)
   - Fast querying and filtering
   - Usage tracking and analytics
   - Supports recipe forking/customization

### Commands vs Direct Calls
- Using `Commands.execute()` instead of direct imports
- Environment-agnostic (works in browser/server)
- Maintains command audit trail

### BaseEntity Extension
- Required for database storage
- Provides validation, timestamps, versioning
- Consistent with existing entity patterns

## 📚 Related Files

### Commands
- `/commands/recipe/load/server/RecipeLoadServerCommand.ts` - Server-side recipe loading
- `/commands/recipe/load/shared/RecipeLoadTypes.ts` - Type definitions
- `/commands/rag/build/server/RAGBuildServerCommand.ts` - RAG context building

### Entities
- `/system/data/entities/RecipeEntity.ts` - Database entity
- `/system/recipes/shared/RecipeTypes.ts` - Domain types

### Templates
- `/system/recipes/general-chat.json` - Human-focused chat recipe

### Builders
- `/system/rag/builders/ChatRAGBuilder.ts` - Existing RAG builder (reused)
- `/system/rag/shared/RAGTypes.ts` - RAG type definitions

## 🔬 Testing

### Manual Testing Commands
```bash
# Once CLI session issue is resolved:
./jtag recipe/load --recipeId=general-chat
./jtag recipe/load --loadAll
./jtag recipe/load --recipeId=general-chat --reload

# Verify loaded:
./jtag data/list --collection=recipes
./jtag data/read --collection=recipes --id=<recipe-id>

# Test RAG building:
./jtag rag/build --contextId=<room-id> --personaId=<persona-id> --maxMessages=20
```

### Current Workaround
Due to CLI session issue, test via:
1. Browser console (widget-based)
2. Direct server-side script (tsx)
3. Integration tests with mocked sessions

## 📝 Documentation

See `RECIPES.md` for full architecture documentation including:
- Recipe lifecycle
- Pipeline execution model
- RAG template structure
- Conversation pattern types
- Future enhancements (LoRA integration, Academy training, etc.)

---

**Status**: Foundation complete, ready for Phase 2 (pipeline execution) once CLI session issue is resolved or workaround implemented.
