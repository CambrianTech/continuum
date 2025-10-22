# GIT WORKFLOW: CONVERSATIONAL CODE COLLABORATION

**Pattern**: Chat Room + Git Integration + AI Assistance
**Complexity**: Medium | **Participants**: Team + GitSentinel + LibrarianAI
**Status**: Design concept demonstrating recipe pattern

---

## CORE CONCEPT

Git workflow becomes a **chat room** where:
- **Git hooks** post messages (via GitSentinel system persona)
- **LibrarianAI** offers helpful suggestions
- **Team members** collaborate conversationally
- **Recipe** orchestrates the workflow

---

## THE CONVERSATION

### Setup: Creating Git Workflow Room

```
👤 Joel: "@planner-ai Set up a git workflow room for our team"

🤖 PlannerAI: "Creating git-workflow room with:
              - GitSentinel (converts git hooks to chat messages)
              - LibrarianAI (helpful code assistant)
              - Recipe: git-workflow-collaboration.json

              Integrating with git hooks..."

[System creates room, personas, recipe]

✅ Room created: #git-workflow
✅ Git hooks configured
✅ Sentinel and Librarian active
```

### Scenario 1: Pre-commit Hook Failure

```
[Developer attempts commit with lint errors]

🤖 GitSentinel: "⚠️ Pre-commit hook failed

                **Lint Errors** (3):
                - src/commands/example/ExampleCommand.ts:45
                  - Missing return type annotation
                - src/widgets/example/ExampleWidget.ts:122
                  - Unused variable 'temp'
                - src/utils/helper.ts:67
                  - Prefer const over let

                Commit blocked."

📚 LibrarianAI: "I can help fix these! Would you like me to:

                1. Add the return type annotation (I see it should be Promise<void>)
                2. Remove the unused variable
                3. Change let to const

                Or I can create a quick fix script?"

👤 Developer: "Yes please, fix them all"

📚 LibrarianAI: "Applying fixes...

                [Executes: lint/fix command]

                ✅ All 3 issues resolved!
                ✅ Ready to commit"

[Developer commits successfully]

🤖 GitSentinel: "✅ Commit successful: feature/add-example-command

                Changed files: 3
                Lines added: 45
                Lines removed: 12"
```

### Scenario 2: Merge Conflict Assistance

```
[Developer pulls from main, gets merge conflict]

🤖 GitSentinel: "⚠️ Merge conflict detected

                **Conflicting files**:
                - src/shared/types.ts (lines 45-67)

                Both you and @teammate modified the same section."

📚 LibrarianAI: "I've analyzed the conflict:

                **Your changes**: Added 'NewCommandType'
                **Teammate's changes**: Added 'AnotherCommandType'

                These can be merged without issues. Both types are
                independent. I recommend accepting both changes.

                Shall I resolve this?"

👤 Developer: "Show me the resolution first"

📚 LibrarianAI: "Here's the merged result:

                ```typescript
                export type CommandType =
                  | 'NewCommandType'      // Your addition
                  | 'AnotherCommandType'  // Teammate's addition
                  | 'ExistingType';
                ```

                Both types preserved, no conflicts."

👤 Developer: "Looks good, apply it"

📚 LibrarianAI: [Applies merge resolution]

                ✅ Conflict resolved
                ✅ Ready to commit merge"
```

### Scenario 3: Code Review Request

```
👤 Developer: "Can someone review my PR for the new widget system?"

📚 LibrarianAI: "I'll do an initial automated review while waiting for human review.

                [Analyzing PR...]

                **Automated Review**:
                ✅ All tests pass
                ✅ Type safety looks good
                ✅ Documentation complete
                ⚠️ One suggestion: Consider adding error boundaries

                I can notify @reviewer when they're available."

👤 Reviewer: "Thanks Librarian! Let me take a look..."

[Human review proceeds with LibrarianAI available for questions]
```

### Scenario 4: Team Conventions Question

```
👤 New Developer: "What's our convention for naming command files?"

📚 LibrarianAI: "Based on the codebase patterns:

                **Command File Naming**:
                - Format: `<Noun><Verb>Command.ts`
                - Examples: `DataListCommand.ts`, `RoomCreateCommand.ts`
                - Location: `commands/<domain>/<action>/`

                **Directory Structure**:
                ```
                commands/<domain>/<action>/
                ├── shared/<Name>Types.ts
                ├── server/<Name>ServerCommand.ts
                └── browser/<Name>BrowserCommand.ts
                ```

                Would you like me to generate a template for your command?"

👤 New Developer: "Yes, for a 'user/archive' command"

📚 LibrarianAI: [Generates scaffold]

                "Created structure at commands/user/archive/
                Files ready for implementation!"
```

---

## RECIPE DEFINITION

```json
{
  "uniqueId": "git-workflow-collaboration",
  "name": "Git Workflow Collaboration",
  "description": "Git-aware team collaboration with AI assistance",

  "participants": {
    "humans": "team-members",
    "ai": [
      {
        "id": "git-sentinel",
        "role": "Git hook message relay",
        "systemPersona": true,
        "triggers": ["git-hook-events"]
      },
      {
        "id": "librarian-ai",
        "role": "Helpful code assistant",
        "proactive": true,
        "capabilities": ["code-analysis", "conflict-resolution", "scaffolding"]
      }
    ]
  },

  "integrations": {
    "git": {
      "hooks": [
        "pre-commit",
        "post-commit",
        "pre-push",
        "post-merge"
      ],
      "hookHandler": "git-sentinel"
    }
  },

  "pipeline": [
    {
      "name": "git-hook-to-message",
      "trigger": { "type": "git-hook" },
      "command": "chat/send",
      "params": {
        "senderId": "git-sentinel",
        "roomId": "$roomId",
        "content": "$hookMessage",
        "metadata": {
          "type": "git-event",
          "hook": "$hookName",
          "success": "$hookSuccess"
        }
      }
    },

    {
      "name": "librarian-assistance",
      "trigger": { "type": "chat:message-received", "filter": { "metadata.type": "git-event", "success": false } },
      "command": "rag/build",
      "params": {
        "sources": ["codebase", "team-conventions", "recent-fixes"],
        "context": "$message.content"
      },
      "outputTo": "ragContext"
    },

    {
      "name": "offer-help",
      "command": "ai/generate",
      "params": {
        "personaId": "librarian-ai",
        "prompt": "Analyze this git issue and offer helpful suggestions: $message.content",
        "context": "$ragContext"
      },
      "outputTo": "helpMessage"
    },

    {
      "name": "post-help",
      "command": "chat/send",
      "params": {
        "senderId": "librarian-ai",
        "roomId": "$roomId",
        "content": "$helpMessage"
      }
    }
  ],

  "rag": {
    "sources": [
      "git-status",
      "recent-commits",
      "branch-info",
      "team-conventions",
      "codebase-patterns",
      "common-fixes"
    ],
    "buildOn": ["user-question", "git-event"]
  }
}
```

---

## SYSTEM PERSONAS

### GitSentinel (System Persona)

**Purpose**: Translate git hooks into chat messages

**Behavior**:
- Monitors git hooks
- Posts formatted messages to chat
- Never engages in conversation
- Pure event relay

**Example Messages**:
- "⚠️ Pre-commit hook failed: [details]"
- "✅ Commit successful: [commit info]"
- "⚠️ Merge conflict detected: [files]"
- "✅ Push successful: [branch → remote]"

### LibrarianAI (Helpful Assistant)

**Purpose**: Proactive code collaboration assistant

**Capabilities**:
- Code analysis and suggestions
- Merge conflict resolution
- Convention guidance
- Scaffolding generation
- Error explanation

**RAG Context**:
- Team coding conventions
- Codebase patterns
- Recent similar fixes
- Git history

**Personality**: Helpful, clear, technically precise

---

## COMPONENTS

### New Commands

#### `git/hook-relay`
**Purpose**: Relay git hook events to chat
**Params**:
```typescript
interface GitHookRelayParams extends CommandParams {
  hookName: string;           // 'pre-commit', 'post-commit', etc.
  success: boolean;
  message: string;
  details?: Record<string, any>;
}
```

#### `code/analyze`
**Purpose**: AI analyzes code for suggestions
**Params**:
```typescript
interface CodeAnalyzeParams extends CommandParams {
  files: string[];
  analysisType: 'lint' | 'security' | 'performance' | 'patterns';
}
```

#### `code/scaffold`
**Purpose**: Generate code templates
**Params**:
```typescript
interface CodeScaffoldParams extends CommandParams {
  templateType: string;       // 'command', 'widget', 'entity'
  name: string;
  location: string;
}
```

### Git Hook Integration

**Approach**: Git hooks execute `./jtag git/hook-relay` command

**Example Pre-commit Hook**:
```bash
#!/bin/bash
# .git/hooks/pre-commit

# Run lint
if ! npm run lint; then
  ./jtag git/hook-relay \
    --hookName="pre-commit" \
    --success=false \
    --message="Pre-commit hook failed: Lint errors detected"
  exit 1
fi

./jtag git/hook-relay \
  --hookName="pre-commit" \
  --success=true \
  --message="Pre-commit checks passed"
exit 0
```

---

## BENEFITS

### For Developers
- **Git events visible** to whole team in chat
- **AI assistance** for common issues
- **Conventions documented** and enforced
- **Faster onboarding** (Librarian answers questions)

### For Teams
- **Shared awareness** of git activity
- **Consistent workflows** via recipe
- **Knowledge preservation** in RAG
- **Async collaboration** through chat

### For the System
- **Same architecture** as all other rooms
- **Recipe-driven** workflow
- **AI as team member** (first-class citizen)
- **Extensible** to other tools (CI/CD, issue tracking, etc.)

---

## EXTENSIONS

**Similar patterns work for**:
- **CI/CD pipelines** → Build status to chat
- **Issue tracking** → Jira/GitHub issues to chat
- **Deployment** → Deploy events + rollback assistance
- **Monitoring** → Alerts + AI diagnosis
- **Code generation** → AI pair programming in chat

**All use the same recipe pattern: Chat Room + Integration + AI Assistance**

---

## CONCLUSION

Git workflow isn't a special system - it's a **chat room with a recipe** that:
- Integrates git hooks via GitSentinel
- Provides AI assistance via LibrarianAI
- Orchestrates team collaboration via recipe

**Same primitives, different pattern.**
