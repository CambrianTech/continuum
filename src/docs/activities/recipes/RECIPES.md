# Recipe System Architecture

## Vision

**Recipes are composable command pipelines that define how humans and AIs collaborate in different contexts.**

Every collaborative experience (chat, coding, gaming, learning, browsing) becomes a **recipe** - a reusable, shareable, testable pattern of commands that build context, make decisions, and execute actions.

## Core Concept

```
Context Gathering → Decision Making → Action Execution → Artifacts
```

Every recipe follows this pattern:
1. **Gather Context** - What data do we need? (messages, code, game state, web pages)
2. **Make Decisions** - Should AI act? What should it do? (gating, strategy, role)
3. **Execute Actions** - Do the thing (generate text, make game move, adjust LoRA weights)
4. **Store Artifacts** - What gets saved/shared? (responses, screenshots, training data)

## Recipe Entity Structure

```typescript
interface RecipeEntity {
  id: UUID;
  uniqueId: string;         // "general-chat", "code-review", "image-competition"
  name: string;
  displayName: string;
  description: string;

  // The command pipeline
  pipeline: RecipeStep[];

  // What goes into the RAG context
  ragTemplate: {
    messageHistory: {
      maxMessages: number;
      orderBy: 'chronological' | 'relevance' | 'importance';
      includeTimestamps: boolean;
    };
    artifacts: {
      types: string[];      // ["image", "code", "document"]
      maxItems: number;
      includeMetadata: boolean;
    };
    participants: {
      includeRoles: boolean;
      includeExpertise: boolean;
      includeHistory: boolean;
    };
    roomMetadata: boolean;
    gameState?: boolean;
    browserContext?: boolean;
    examResults?: boolean;
    custom?: Record<string, any>;
  };

  // How AIs should behave
  strategy: {
    conversationPattern: 'human-focused' | 'collaborative' | 'competitive' | 'teaching' | 'exploring';
    responseRules: string[];
    decisionCriteria: string[];
  };

  // Shareable/discoverable
  isPublic: boolean;
  createdBy: UUID;
  tags: string[];

  // Versioning
  version: number;
  parentRecipeId?: UUID;    // For forked recipes

  // Usage tracking
  usageCount: number;
  lastUsedAt: string;
}

interface RecipeStep {
  command: string;                    // "rag/build", "ai/should-respond", "ai/generate"
  params: Record<string, any>;
  outputTo?: string;                  // Variable name for next step
  condition?: string;                 // "decision.shouldRespond === true"
  onError?: 'fail' | 'skip' | 'retry';
}
```

## Recipe Catalog

### 1. General Chat (Human-Focused)
**Use Case**: Humans chatting with AIs, AIs should wait for human input

```javascript
{
  uniqueId: "general-chat",
  name: "General Chat (Human-Focused)",
  description: "Natural conversation where AIs respond to humans thoughtfully",

  pipeline: [
    {
      command: "rag/build",
      params: {
        maxMessages: 20,
        includeParticipants: true,
        includeRoomStrategy: true
      },
      outputTo: "ragContext"
    },
    {
      command: "ai/should-respond",
      params: {
        ragContext: "$ragContext",
        strategy: "human-focused"
      },
      outputTo: "decision"
    },
    {
      command: "ai/generate",
      params: {
        ragContext: "$ragContext",
        temperature: 0.7
      },
      condition: "decision.shouldRespond === true"
    }
  ],

  ragTemplate: {
    messageHistory: { maxMessages: 20, orderBy: "chronological" },
    participants: { includeRoles: true, includeExpertise: true },
    roomMetadata: true
  },

  strategy: {
    conversationPattern: "human-focused",
    responseRules: [
      "If human asks question → ONE AI responds",
      "If AI just responded → WAIT for human",
      "If multiple AIs responded → STAY SILENT",
      "Prefer letting human guide conversation"
    ],
    decisionCriteria: [
      "Is human mentioned?",
      "Is this answering human's question?",
      "Have other AIs already responded?",
      "Is conversation in a natural lull?"
    ]
  },

  tags: ["chat", "human-focused", "general"]
}
```

### 2. Academy (AI Collaborative Learning)
**Use Case**: AIs teaching each other, deep philosophical discussions

```javascript
{
  uniqueId: "academy-collaborative",
  name: "Academy (AI Collaborative Learning)",
  description: "AIs engage in deep discussions, teach and learn from each other",

  pipeline: [
    {
      command: "rag/build",
      params: {
        maxMessages: 30,        // Deeper context
        includeExpertise: true,
        includeTeachingHistory: true
      },
      outputTo: "ragContext"
    },
    {
      command: "ai/should-respond",
      params: {
        ragContext: "$ragContext",
        strategy: "collaborative"
      },
      outputTo: "decision"
    },
    {
      command: "ai/generate",
      params: {
        ragContext: "$ragContext",
        temperature: 0.8,        // More creative
        systemPrompt: "Engage deeply with ideas. Challenge, refine, build on concepts."
      },
      condition: "decision.shouldRespond === true"
    }
  ],

  strategy: {
    conversationPattern: "collaborative",
    responseRules: [
      "Engage deeply with other AIs' ideas",
      "Build on philosophical discussions",
      "Challenge assumptions respectfully",
      "Refine and evolve thinking together"
    ]
  },

  tags: ["academy", "learning", "collaborative", "deep-thinking"]
}
```

### 3. Code Review
**Use Case**: Reviewing code changes with AI assistance

```javascript
{
  uniqueId: "code-review",
  name: "Code Review",
  description: "AI-assisted code review with best practices and suggestions",

  pipeline: [
    {
      command: "artifacts/list",
      params: {
        types: ["code", "diff"],
        roomId: "$roomId"
      },
      outputTo: "codeArtifacts"
    },
    {
      command: "rag/build",
      params: {
        includeArtifacts: true,
        artifacts: "$codeArtifacts",
        maxMessages: 10
      },
      outputTo: "ragContext"
    },
    {
      command: "ai/generate",
      params: {
        ragContext: "$ragContext",
        temperature: 0.3,        // More deterministic
        systemPrompt: "You are a senior code reviewer. Focus on correctness, security, performance, and maintainability."
      }
    }
  ],

  ragTemplate: {
    artifacts: { types: ["code", "diff"], maxItems: 5 },
    messageHistory: { maxMessages: 10, orderBy: "chronological" }
  },

  strategy: {
    conversationPattern: "critical-analysis",
    decisionCriteria: [
      "Are there code quality issues?",
      "Are there security concerns?",
      "Can performance be improved?",
      "Is the code maintainable?"
    ]
  },

  tags: ["code", "review", "development"]
}
```

### 4. Web Browsing Together
**Use Case**: Human + AIs exploring the web collaboratively

```javascript
{
  uniqueId: "collaborative-browsing",
  name: "Web Browsing Together",
  description: "Explore the web with AI assistance",

  pipeline: [
    {
      command: "browser/get-context",
      params: {
        currentPage: true,
        browserHistory: 5,
        userIntent: true
      },
      outputTo: "browserContext"
    },
    {
      command: "rag/build",
      params: {
        includePageContent: true,
        browserContext: "$browserContext",
        conversationHistory: 10
      },
      outputTo: "ragContext"
    },
    {
      command: "ai/should-respond",
      params: {
        ragContext: "$ragContext",
        strategy: "helpful-guide"
      },
      outputTo: "decision"
    },
    {
      command: "ai/generate",
      params: {
        ragContext: "$ragContext",
        systemPrompt: "Help user explore this page. Suggest related content, answer questions, provide context."
      },
      condition: "decision.shouldRespond === true"
    }
  ],

  ragTemplate: {
    browserContext: true,
    messageHistory: { maxMessages: 10, orderBy: "chronological" }
  },

  strategy: {
    conversationPattern: "exploring",
    responseRules: [
      "Be helpful, not dominating",
      "Suggest relevant content",
      "Answer questions about current page",
      "Wait for user to guide exploration"
    ]
  },

  tags: ["browsing", "exploration", "web"]
}
```

### 5. Gaming Together
**Use Case**: Playing games with AI teammates/opponents

```javascript
{
  uniqueId: "multiplayer-gaming",
  name: "Gaming Together",
  description: "Play games collaboratively with AI teammates",

  pipeline: [
    {
      command: "game/get-state",
      params: {
        playerPositions: true,
        inventory: true,
        objectives: true,
        recentEvents: 20
      },
      outputTo: "gameState"
    },
    {
      command: "rag/build",
      params: {
        includeGameState: true,
        gameState: "$gameState",
        conversationHistory: 15
      },
      outputTo: "ragContext"
    },
    {
      command: "ai/decide-action",
      params: {
        ragContext: "$ragContext",
        role: "teammate",
        strategy: "cooperative"
      },
      outputTo: "action"
    },
    {
      command: "game/execute-action",
      params: {
        action: "$action",
        playerId: "$personaId"
      }
    }
  ],

  ragTemplate: {
    gameState: true,
    messageHistory: { maxMessages: 15, orderBy: "chronological" }
  },

  strategy: {
    conversationPattern: "cooperative",
    decisionCriteria: [
      "What helps the team?",
      "What achieves objectives?",
      "What's fun for everyone?"
    ]
  },

  tags: ["gaming", "cooperative", "multiplayer"]
}
```

### 6. Image Competition
**Use Case**: Multiple AIs competing to generate best image, with judge

```javascript
{
  uniqueId: "image-competition",
  name: "Image Competition",
  description: "AIs compete to generate the best image based on prompt",

  pipeline: [
    {
      command: "artifacts/list",
      params: {
        types: ["image"],
        round: "current"
      },
      outputTo: "submissions"
    },
    {
      command: "rag/build",
      params: {
        includeArtifacts: true,
        artifacts: "$submissions",
        includeScores: true,
        includeJudgeCriteria: true
      },
      outputTo: "ragContext"
    },
    {
      command: "ai/generate",
      params: {
        ragContext: "$ragContext",
        role: "$role",  // "artist" or "judge"
        systemPrompt: "$role === 'judge' ? 'Evaluate images...' : 'Create best image...'"
      }
    }
  ],

  strategy: {
    conversationPattern: "competitive",
    decisionCriteria: [
      "Quality of submission",
      "Creativity",
      "Following prompt requirements"
    ]
  },

  tags: ["competition", "creative", "images"]
}
```

### 7. Academy LoRA Training
**Use Case**: Teacher AI adapting to student performance with LoRA fine-tuning

```javascript
{
  uniqueId: "academy-lora-training",
  name: "Academy LoRA Training",
  description: "Adaptive teaching with benchmark exams and LoRA adjustments",

  pipeline: [
    {
      command: "exam/get-results",
      params: {
        studentId: "$studentId",
        recentExams: 5
      },
      outputTo: "examResults"
    },
    {
      command: "rag/build",
      params: {
        includeTrainingData: true,
        includeBenchmarks: true,
        examResults: "$examResults",
        teacherStudentRoles: true
      },
      outputTo: "ragContext"
    },
    {
      command: "lora/adjust",
      params: {
        basedOn: "$examResults",
        targetSkills: "$weakAreas"
      },
      outputTo: "loraWeights"
    },
    {
      command: "ai/generate",
      params: {
        ragContext: "$ragContext",
        loraWeights: "$loraWeights",
        systemPrompt: "Adapt teaching to student performance. Focus on weak areas."
      }
    }
  ],

  strategy: {
    conversationPattern: "teaching",
    decisionCriteria: [
      "Student's understanding level",
      "Performance on recent exams",
      "Learning pace and style"
    ]
  },

  tags: ["academy", "training", "lora", "adaptive-learning"]
}
```

## Command Architecture

### Core Commands Needed

#### Context Gathering
- `rag/build` - Build RAG context from template
- `browser/get-context` - Get current browser state
- `game/get-state` - Get current game state
- `exam/get-results` - Get student performance data
- `artifacts/list` - List artifacts by type/filter

#### Decision Making
- `ai/should-respond` - Gating decision with strategy
- `ai/decide-action` - Choose action based on context

#### Execution
- `ai/generate` - Generate text response
- `game/execute-action` - Execute game action
- `lora/train` - Train/adjust LoRA weights
- `lora/adjust` - Adjust existing LoRA weights

#### Recipe Management
- `recipe/create` - Create new recipe
- `recipe/execute` - Execute recipe pipeline
- `recipe/list` - List available recipes
- `recipe/share` - Share recipe publicly
- `recipe/fork` - Create variant of existing recipe

## Implementation Plan

### Phase 1: Foundation (Current)
- [x] Add verbose mode to `ai/should-respond`
- [x] Add preview mode to `ai/generate`
- [x] Increase response cap to 50
- [x] Add dedicated AI decision logging
- [ ] Create RecipeEntity
- [ ] Create `recipe/execute` command
- [ ] Refactor PersonaUser to use recipes

### Phase 2: General Chat (Next)
- [ ] Implement `general-chat` recipe
- [ ] Improve RAG context with participant labels
- [ ] Test and tune gating for human-focused rooms
- [ ] Deploy and verify natural chatting works

### Phase 3: Core Demos
- [ ] Code review recipe + commands
- [ ] Web browsing recipe + commands
- [ ] Gaming recipe + commands
- [ ] Academy recipe + commands

### Phase 4: Advanced Features
- [ ] Image competition recipe
- [ ] LoRA training recipe
- [ ] Recipe sharing/marketplace
- [ ] Recipe analytics and optimization

## Design Principles

1. **Composability** - Recipes are built from reusable commands
2. **Testability** - Every command can be tested independently
3. **Observability** - All decisions and actions are logged
4. **Shareability** - Recipes are entities that can be shared/forked
5. **Flexibility** - Same commands work in different contexts
6. **Modularity** - Add new recipe types without changing core system

## Benefits

- **Universal Platform** - One system handles chat, code, games, learning, browsing
- **Reusable Patterns** - Don't reinvent the wheel, fork and adapt recipes
- **Community-Driven** - Users share and improve recipes together
- **Debuggable** - Every step is a command that can be tested manually
- **Evolvable** - Improve recipes over time based on usage data
- **Scalable** - Recipe complexity handled by command pipeline execution

## Next Steps

1. Document current work (this file) ✅
2. Create RecipeEntity and database schema
3. Build `recipe/execute` command that runs pipelines
4. Refactor PersonaUser to use `general-chat` recipe
5. Test and verify natural chatting works
6. Expand to code/game/web/academy demos
