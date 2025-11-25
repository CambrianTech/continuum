# Dumb Sentinels - Single-Purpose Agent Bots

## Philosophy: Dumb = Good

**Smart AIs** try to do everything → get confused, make mistakes, need complex prompts

**Dumb Sentinels** do ONE thing → simple, reliable, predictable

```
Smart AI:  "I can help with code, tests, docs, debugging, architecture..."
           → Often confused about what you want
           → Needs clarification
           → Unpredictable results

Dumb Sentinel: "I fix imports. That's it."
               → Always knows what to do
               → No ambiguity
               → Predictable results
```

---

## The Dumb Sentinel Pattern

### Core Principle: If-This-Then-That (IFTTT)

```typescript
interface DumbSentinel {
  name: string;
  trigger: SimpleTrigger;      // What wakes it up?
  action: SimpleAction;         // What does it do?
  tools: string[];              // Limited toolset
  // NO complex decision-making
  // NO conversation
  // NO learning
  // JUST: trigger → action → done
}
```

---

## Example: ImportFixerSentinel (The Dumbest One)

```typescript
const ImportFixerSentinel: DumbSentinel = {
  name: 'ImportFixer',

  // Trigger: User says "@ImportFixer" in chat
  trigger: {
    type: 'mention',
    pattern: /@ImportFixer (.+) -> (.+)/  // "@ImportFixer old/path -> new/path"
  },

  // Action: Find and replace imports
  action: async (match) => {
    const oldPath = match[1];  // "old/path"
    const newPath = match[2];  // "new/path"

    // 1. Find all files with old import
    const files = await grep(`from ['"]${oldPath}['"]`);

    // 2. Replace in each file
    for (const file of files) {
      await replace(file, oldPath, newPath);
    }

    // 3. Verify compilation
    const compiled = await exec('npx tsc --noEmit');

    // 4. Report
    return {
      message: `Fixed ${files.length} files`,
      success: compiled.exitCode === 0
    };
  },

  tools: ['grep', 'replace', 'exec']  // Only needs 3 tools
};
```

**Usage:**
```
Joel: "@ImportFixer system/core/Commands -> system/core/shared/Commands"
ImportFixer: "Fixed 47 files ✅"
```

**That's it. No intelligence. No decisions. Just pattern matching and text replacement.**

---

## More Dumb Sentinels

### 1. **TypeErrorFixer** (Extremely Dumb)

```typescript
const TypeErrorFixer: DumbSentinel = {
  name: 'TypeErrorFixer',

  trigger: {
    type: 'error-log',
    pattern: /TS\d+: (.+)/  // TypeScript errors
  },

  action: async (error) => {
    // 1. Run tsc and capture errors
    const errors = await exec('npx tsc --noEmit 2>&1 | grep "TS"');

    // 2. For each error, apply known fixes
    for (const err of errors) {
      if (err.includes('missing import')) {
        await addMissingImport(err);
      }
      else if (err.includes('unused variable')) {
        await removeUnusedVariable(err);
      }
      // etc - just a lookup table of known fixes
    }

    return { fixed: errors.length };
  },

  tools: ['exec', 'edit']
};
```

---

### 2. **UnusedImportCleaner** (Very Dumb)

```typescript
const UnusedImportCleaner: DumbSentinel = {
  name: 'UnusedImportCleaner',

  trigger: {
    type: 'scheduled',
    cron: '0 2 * * *'  // Every night at 2am
  },

  action: async () => {
    // 1. Run eslint with unused-imports rule
    const result = await exec('npx eslint --fix src/');

    // 2. That's it - eslint does the work
    return { message: 'Cleaned unused imports' };
  },

  tools: ['exec']
};
```

---

### 3. **TestRunner** (Super Dumb)

```typescript
const TestRunner: DumbSentinel = {
  name: 'TestRunner',

  trigger: {
    type: 'file-change',
    pattern: '**/*.ts'
  },

  action: async (changedFile) => {
    // 1. Find test file for changed file
    const testFile = changedFile.replace('.ts', '.test.ts');

    // 2. Run that test
    const result = await exec(`npm test ${testFile}`);

    // 3. Report pass/fail
    return {
      message: result.exitCode === 0 ? '✅ Tests pass' : '❌ Tests fail',
      success: result.exitCode === 0
    };
  },

  tools: ['exec']
};
```

---

### 4. **LogWatcher** (Dumbest Possible)

```typescript
const LogWatcher: DumbSentinel = {
  name: 'LogWatcher',

  trigger: {
    type: 'log-line',
    pattern: /❌|ERROR|FATAL/
  },

  action: async (logLine) => {
    // Just copy error to chat room
    await postToChatRoom('debug', {
      text: `🚨 Error detected:\n${logLine}`
    });

    return { message: 'Posted to #debug' };
  },

  tools: ['chat']
};
```

---

## Implementation: Dumb = Simple

```typescript
/**
 * Dumb Sentinel - No complex AI, just trigger → action
 */
class DumbSentinel {
  name: string;
  trigger: TriggerConfig;
  action: (match: any) => Promise<ActionResult>;
  tools: ToolRegistry;

  /**
   * Main loop - just watch trigger and execute action
   */
  async run(): Promise<void> {
    while (true) {
      // Wait for trigger
      const match = await this.trigger.wait();

      if (match) {
        console.log(`🤖 ${this.name}: Triggered`);

        try {
          // Execute action (synchronously, no parallelism)
          const result = await this.action(match);

          // Report result
          console.log(`✅ ${this.name}: ${result.message}`);

        } catch (error) {
          console.error(`❌ ${this.name}: Failed -`, error);
        }
      }
    }
  }
}
```

**No PersonaUser complexity. No conversation state. No LLM calls. Just trigger → action → done.**

---

## Why Dumb Sentinels Are Better

### 1. **Predictable**
```
Smart AI:  "I'll try to fix the imports... maybe... if I understand correctly..."
Dumb Bot: "Pattern matched. Replacing. Done."
```

### 2. **Fast**
```
Smart AI:  5-10 seconds (LLM call)
Dumb Bot: 0.1 seconds (regex + file ops)
```

### 3. **Cheap**
```
Smart AI:  $0.01 per task (API calls)
Dumb Bot: $0.00 (local execution)
```

### 4. **Debuggable**
```
Smart AI:  "Why did it do that?" → check prompt, check LLM response, check...
Dumb Bot: "Why did it do that?" → read 10 lines of code
```

### 5. **Reliable**
```
Smart AI:  Works 80% of the time (depends on prompt quality, LLM mood)
Dumb Bot: Works 100% of the time (deterministic logic)
```

---

## When to Use Which?

### Use Dumb Sentinel When:
- ✅ Task is repetitive and well-defined
- ✅ Pattern matching is sufficient
- ✅ Speed matters
- ✅ Zero cost is important
- ✅ Determinism is required

### Use Smart AI (PersonaUser) When:
- ✅ Task requires understanding context
- ✅ Natural language interaction needed
- ✅ Creative problem-solving required
- ✅ Multiple valid approaches exist
- ✅ Learning from examples is valuable

---

## Example: Import Migration

**Dumb Sentinel Approach:**
```typescript
// Trigger: @ImportFixer Commands -> shared/Commands
// Action: grep → replace → compile → done
// Time: 2 seconds
// Cost: $0
// Reliability: 100%
```

**Smart AI Approach:**
```typescript
// Trigger: "We moved Commands, can you update imports?"
// Action: understand intent → plan migration → ask clarification → execute → verify
// Time: 30-60 seconds
// Cost: $0.05-0.10 (API calls)
// Reliability: 90% (might misunderstand, need retries)
```

**For this task, Dumb Sentinel wins every time.**

---

## Architecture: Hybrid System

```
Continuum System
├── PersonaUsers (Smart AIs)
│   ├── CodeAI - Answers code questions
│   ├── PlannerAI - Architecture discussions
│   └── GeneralAI - General help
│
└── Sentinels (Dumb Bots)
    ├── ImportFixer - Fix import paths
    ├── TypeErrorFixer - Fix type errors
    ├── TestRunner - Run tests on change
    ├── LogWatcher - Monitor error logs
    └── UnusedImportCleaner - Clean unused imports
```

**Smart AIs for conversation, Dumb Sentinels for automation.**

---

## Implementation Priority

### Phase 1: Prove The Pattern (1 day)
1. Implement DumbSentinel base class
2. Implement ImportFixerSentinel
3. Test with real import migration
4. Verify it's faster/simpler than smart AI

### Phase 2: Add More Dumb Bots (2 days)
5. TypeErrorFixer
6. TestRunner
7. LogWatcher
8. UnusedImportCleaner

### Phase 3: Make Them Discoverable (1 day)
9. `./jtag sentinels/list` - Show all sentinels
10. `./jtag sentinels/trigger <name> <args>` - Manual trigger
11. `@SentinelName` mention support in chat
12. Auto-trigger based on events

---

## The Vision: Janitor Bots

Think of Dumb Sentinels as **janitor bots** for your codebase:

```
ImportFixer:        "I clean up import statements"
TypeErrorFixer:     "I fix simple type errors"
TestRunner:         "I run tests when files change"
LogWatcher:         "I watch for errors in logs"
UnusedImportCleaner: "I remove unused imports every night"
```

**They don't need to be smart. They just need to be reliable and do their ONE job well.**

---

## Comparison: Me (Claude Code) vs Dumb Sentinel

**What I Do (Smart AI):**
```
Joel: "Fix the import paths"
Me: [Thinks deeply]
    [Analyzes codebase]
    [Spawns Task agent]
    [Agent reads 176 lines, then 546 lines]
    [Makes 14+ tool calls]
    [Returns comprehensive report]
    "Done! Fixed 47 files, found 2 edge cases..."

Time: 30-60 seconds
Intelligence: High
Cost: API calls
Reliability: 90%
```

**What ImportFixer Does (Dumb Sentinel):**
```
Joel: "@ImportFixer old/path -> new/path"
ImportFixer: grep old/path → sed s/old/new/ → tsc --noEmit
             "Fixed 47 files ✅"

Time: 2 seconds
Intelligence: Zero
Cost: $0
Reliability: 100%
```

**For simple tasks, dumb wins.**

---

## The Perfect Combo

```
Joel: "We need to refactor the PersonaUser architecture"
↓
CodeAI (Smart): "I'd suggest these patterns... [detailed analysis]"
Joel: "Great, let's do it"
↓
Joel: "@ImportFixer PersonaUser -> user/PersonaUser"
ImportFixer (Dumb): "Fixed 23 files ✅"
↓
TestRunner (Dumb): [Auto-triggered] "✅ All tests pass"
↓
LogWatcher (Dumb): [Auto-triggered] "No errors detected"
↓
Joel: "Perfect!"
```

**Smart AI for thinking, Dumb Sentinels for doing.**

This is the way.
