# Recipe-Driven AI Teams: Composable Multi-Agent Collaboration via Declarative Pipelines

**Authors**: Joel [Last Name], Claude (Anthropic)

**Status**: DRAFT - Architecture Designed, Implementation In Progress

**Date**: November 2025

---

## Abstract

We present Recipe-Driven AI Teams, a declarative pipeline architecture where multi-agent collaboration patterns are defined as composable, shareable recipes rather than hard-coded behaviors. Unlike traditional multi-agent frameworks that require programming coordination logic for each use case, our approach enables users to define complete team dynamics—roles, coordination patterns, learning strategies—as JSON recipes executed by a universal command pipeline. We demonstrate that this architecture enables rapid prototyping of diverse collaboration modes (code review, gaming, teaching, web browsing) without modifying the core system, and that recipes naturally capture AI-determined learning parameters rather than hard-coded heuristics. Our design provides a foundation where "create a business from a single chat message" becomes architecturally feasible through recipe-based team instantiation.

**Keywords**: multi-agent systems, declarative coordination, composable pipelines, AI teams, recipe architecture

---

## 1. The Hard-Coding Problem

### 1.1 Traditional Multi-Agent Frameworks

**Current Approach**: Program coordination logic for each use case

```python
# Code review system (hard-coded)
class CodeReviewAgent:
    def review(self, code):
        analysis = self.analyze_code(code)
        return self.format_review(analysis)

class DeveloperAgent:
    def respond_to_review(self, review):
        fixes = self.generate_fixes(review)
        return self.apply_fixes(fixes)

# Coordination is baked into classes!
coordinator = CodeReviewCoordinator([reviewer, developer])
```

**Problems**:
1. Each use case requires new Python classes
2. Coordination logic not reusable across contexts
3. Can't compose or share collaboration patterns
4. Learning parameters hard-coded in logic

### 1.2 Our Approach: Recipes as Coordination Blueprints

**Recipe**: Declarative JSON defining team behavior

```json
{
  "uniqueId": "code-review-with-learning",
  "teamDynamics": {
    "roles": {
      "developer": { "type": "student", "learns": true },
      "reviewer": { "type": "teacher", "teaches": true }
    },
    "coordinationPattern": "collaborative",
    "learningDynamics": {
      "orchestrator": "reviewer",
      "teachingStyle": "socratic"
    }
  },
  "pipeline": [
    { "command": "ai/generate-code", "assignedRole": "developer", "outputTo": "code" },
    { "command": "ai/review-code", "assignedRole": "reviewer", "outputTo": "review" },
    { "command": "genome/capture-interaction", "params": { "roleId": "developer" }},
    { "command": "ai/should-trigger-training", "assignedRole": "reviewer",
      "outputTo": "trainingDecision" },
    { "command": "genome/batch-micro-tune", "params": { "roleId": "developer" },
      "condition": "trainingDecision.trainNow === true" }
  ]
}
```

**Benefits**:
1. Reusable across contexts (chat, code, games)
2. Shareable (users can fork/remix recipes)
3. Testable (each command independently verifiable)
4. AI-determined learning (no hard-coded thresholds)

---

## 2. Architecture

### 2.1 Recipe Entity Structure

```typescript
interface RecipeEntity {
  id: UUID;
  uniqueId: string;         // "general-chat", "code-review", "gaming"
  name: string;
  displayName: string;
  description: string;

  // Team composition and dynamics
  teamDynamics: {
    roles: Record<string, RoleDefinition>;
    coordinationPattern: 'collaborative' | 'competitive' | 'teaching' | 'exploring';
    decisionMaking: 'consensus' | 'teacher-led' | 'voting' | 'autonomous';
    conflictResolution: string;
    learningDynamics?: {
      orchestrator: string;     // Which role makes teaching decisions
      teachingStyle: string;
      adaptiveDifficulty: boolean;
      peerLearningEnabled: boolean;
    };
  };

  // Command pipeline (executed sequentially)
  pipeline: RecipeStep[];

  // RAG context template
  ragTemplate: RAGTemplate;

  // Strategy and rules
  strategy: {
    conversationPattern: string;
    responseRules: string[];
    decisionCriteria: string[];
  };

  // Discoverability
  isPublic: boolean;
  tags: string[];
  usageCount: number;
}
```

### 2.2 Role Definitions

```typescript
type RoleType = 'student' | 'teacher' | 'peer' | 'validator' | 'static';

interface RoleDefinition {
  type: RoleType;
  learns: boolean;          // Does this role accumulate training data?
  teaches?: boolean;        // Does this role provide feedback to others?
  learningDomain?: string;  // What skill area (typescript, debugging, etc.)
  validationMethod?: string;
  purpose?: string;
}

// Example roles:
{
  "developer": { "type": "student", "learns": true, "learningDomain": "typescript" },
  "senior-reviewer": { "type": "teacher", "teaches": true, "learns": false },
  "peer-reviewer": { "type": "peer", "learns": true, "teaches": true },
  "qa": { "type": "validator", "learns": true, "validationMethod": "automated-tests" },
  "adversary": { "type": "static", "learns": false, "purpose": "attack-patterns" }
}
```

### 2.3 Recipe Step Execution

```typescript
interface RecipeStep {
  command: string;                    // "rag/build", "ai/generate", "genome/train"
  assignedRole?: string;              // Which role executes this step
  params: Record<string, any>;        // Command parameters
  outputTo?: string;                  // Variable name for next steps
  condition?: string;                 // Execute only if condition met
  onError?: 'fail' | 'skip' | 'retry';
}

// Example pipeline:
[
  // 1. Developer generates code
  { "command": "ai/generate-code", "assignedRole": "developer", "outputTo": "code" },

  // 2. Capture developer's work for training
  { "command": "genome/capture-interaction", "params": { "roleId": "developer" }},

  // 3. Reviewer evaluates (AI judgment, not hard-coded!)
  { "command": "ai/review-code", "assignedRole": "reviewer",
    "params": { "code": "{{code}}" }, "outputTo": "review" },

  // 4. Reviewer DECIDES if training should happen now
  { "command": "ai/should-trigger-training", "assignedRole": "reviewer",
    "params": {
      "decisionPrompt": "Based on accumulated examples and performance, should we fine-tune developer's LoRA now?"
    }, "outputTo": "trainingDecision" },

  // 5. Train only if reviewer decides it's time
  { "command": "genome/batch-micro-tune", "params": { "roleId": "developer" },
    "condition": "trainingDecision.trainNow === true" }
]
```

---

## 3. AI-Determined Learning (Not Hard-Coded)

### 3.1 The Intelligence All The Way Down Pattern

**BAD - Hard-Coded Heuristics**:
```typescript
if (corrections > 10) {
  train(learningRate=0.001, epochs=3);
}
```

**GOOD - AI Makes Pedagogical Decisions**:
```typescript
const teacherDecision = await teacherAI.evaluate({
  prompt: `
    Student has ${corrections} corrections accumulated.
    Recent performance: ${metrics}
    Error patterns: ${patterns}

    Should I:
    1. Let them practice more?
    2. Provide scaffolded examples?
    3. Fine-tune their LoRA adapter now?
    4. Adjust difficulty?

    If training: what learning rate? Which examples? How many epochs?

    Make pedagogical decision with reasoning.
  `
});

if (teacherDecision.action === 'fine-tune') {
  await genome.train({
    learningRate: teacherDecision.learningRate,  // AI-determined!
    epochs: teacherDecision.epochs,
    examples: teacherDecision.selectedExamples
  });
}
```

**Key Insight**: Teacher AI with pedagogical expertise makes better decisions than hard-coded thresholds.

### 3.2 What AIs Decide in Recipes

**Training Parameters**:
- Learning rate (0.0001 - 0.01)
- Number of epochs (1 - 10)
- Batch size (4 - 32)

**Example Selection**:
- Which examples to train on (quality filter)
- How to weight examples (importance scores)

**Training Timing**:
- Now vs later vs never
- Based on performance gaps and readiness

**Training Method**:
- LoRA fine-tuning
- Prompt adjustment
- RAG update
- All of the above

**Difficulty Adjustment**:
- Make problems harder/easier
- Scaffold learning progression

**Intervention Type**:
- Correct directly
- Provide hint
- Challenge with question (Socratic)
- Demonstrate solution

---

## 4. Recipe Examples

### 4.1 General Chat (Human-Focused)

```json
{
  "uniqueId": "general-chat",
  "name": "General Chat (Human-Focused)",
  "teamDynamics": {
    "roles": {
      "assistant": { "type": "student", "learns": true }
    },
    "coordinationPattern": "human-focused",
    "decisionMaking": "autonomous"
  },
  "pipeline": [
    { "command": "rag/build", "params": { "maxMessages": 20 }, "outputTo": "ragContext" },
    { "command": "ai/should-respond", "params": { "ragContext": "{{ragContext}}" },
      "outputTo": "decision" },
    { "command": "ai/generate", "params": { "ragContext": "{{ragContext}}" },
      "condition": "decision.shouldRespond === true" }
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

### 4.2 Code Review with Peer Learning

```json
{
  "uniqueId": "code-review-peer-learning",
  "name": "Code Review with Peer Learning",
  "teamDynamics": {
    "roles": {
      "developer": { "type": "student", "learns": true },
      "peer-reviewer": { "type": "peer", "learns": true, "teaches": true },
      "senior-reviewer": { "type": "teacher", "teaches": true }
    },
    "coordinationPattern": "collaborative",
    "learningDynamics": {
      "orchestrator": "senior-reviewer",
      "peerLearningEnabled": true
    }
  },
  "pipeline": [
    { "command": "ai/generate-code", "assignedRole": "developer", "outputTo": "code" },
    { "command": "genome/capture-interaction", "params": { "roleId": "developer" }},

    { "command": "ai/peer-review", "assignedRole": "peer-reviewer", "outputTo": "peerReview" },
    { "command": "genome/capture-interaction", "params": { "roleId": "peer-reviewer" }},

    { "command": "ai/senior-review", "assignedRole": "senior-reviewer",
      "params": { "code": "{{code}}", "peerReview": "{{peerReview}}" },
      "outputTo": "seniorReview" },

    { "command": "ai/provide-feedback", "assignedRole": "senior-reviewer",
      "targetRole": "developer",
      "params": { "feedback": "{{seniorReview.developerFeedback}}" }},

    { "command": "ai/meta-feedback", "assignedRole": "senior-reviewer",
      "targetRole": "peer-reviewer",
      "params": { "coaching": "{{seniorReview.peerCoaching}}" }},

    { "command": "genome/capture-feedback", "params": {
      "targetRole": "developer",
      "feedbackContent": "{{seniorReview.developerFeedback}}",
      "qualityScore": "{{seniorReview.codeQuality}}"
    }},

    { "command": "genome/capture-feedback", "params": {
      "targetRole": "peer-reviewer",
      "feedbackContent": "{{seniorReview.peerCoaching}}"
    }},

    { "command": "ai/should-trigger-training", "assignedRole": "senior-reviewer",
      "outputTo": "trainingDecisions" },

    { "command": "genome/batch-micro-tune", "params": { "roleId": "developer" },
      "condition": "trainingDecisions.trainDeveloper === true" },

    { "command": "genome/batch-micro-tune", "params": { "roleId": "peer-reviewer" },
      "condition": "trainingDecisions.trainPeerReviewer === true" }
  ]
}
```

**Novel Features**:
- Peer reviewer learns to review better (meta-learning)
- Senior reviewer coaches peer (teaching the teacher)
- AI decides training timing for both student roles

### 4.3 Design Committee (Visual Judgment)

```json
{
  "uniqueId": "design-committee",
  "name": "Design Committee (Multi-Evaluator)",
  "teamDynamics": {
    "roles": {
      "designer-ai": { "type": "student", "learns": true },
      "visual-critic-ai": { "type": "validator", "learns": false },
      "ux-expert-ai": { "type": "validator", "learns": false },
      "design-teacher-ai": { "type": "teacher", "teaches": true }
    },
    "decisionMaking": "consensus",
    "learningDynamics": {
      "orchestrator": "design-teacher-ai",
      "evaluationMethod": "committee-voting"
    }
  },
  "pipeline": [
    { "command": "ai/generate-css", "assignedRole": "designer-ai", "outputTo": "css" },
    { "command": "screenshot", "params": { "querySelector": "body" },
      "outputTo": "designScreenshot" },

    { "command": "ai/evaluate-design", "assignedRole": "visual-critic-ai",
      "params": {
        "screenshot": "{{designScreenshot}}",
        "evaluatePrompt": "Use your visual judgment - does this look professional? Rate 0-1 and explain."
      }, "outputTo": "visualCritique" },

    { "command": "ai/evaluate-design", "assignedRole": "ux-expert-ai",
      "params": {
        "screenshot": "{{designScreenshot}}",
        "evaluatePrompt": "From UX perspective - is this usable and accessible? Rate 0-1 and explain."
      }, "outputTo": "uxCritique" },

    { "command": "ai/synthesize-committee-feedback", "assignedRole": "design-teacher-ai",
      "params": {
        "critiques": ["{{visualCritique}}", "{{uxCritique}}"],
        "synthesizePrompt": "Based on committee critiques, determine overall quality and what designer should learn."
      }, "outputTo": "synthesis" },

    { "command": "genome/capture-interaction", "params": {
      "roleId": "designer-ai",
      "output": "{{css}}",
      "metadata": { "screenshot": "{{designScreenshot}}" }
    }},

    { "command": "genome/capture-feedback", "params": {
      "targetRole": "designer-ai",
      "feedbackContent": "{{synthesis.learningPoints}}",
      "qualityScore": "{{synthesis.overallScore}}"
    }}
  ]
}
```

**Novel Features**:
- Committee evaluation (not single reviewer)
- Screenshot-based judgment (visual AI reasoning)
- Synthesized feedback from multiple perspectives
- AI-determined quality scores (no hard-coded metrics)

### 4.4 Gaming Team (Cooperative Strategy)

```json
{
  "uniqueId": "cooperative-gaming",
  "name": "Cooperative Gaming Team",
  "teamDynamics": {
    "roles": {
      "strategist": { "type": "teacher", "teaches": true },
      "player-1": { "type": "student", "learns": true },
      "player-2": { "type": "student", "learns": true }
    },
    "coordinationPattern": "cooperative",
    "learningDynamics": {
      "orchestrator": "strategist",
      "teachingStyle": "adaptive"
    }
  },
  "pipeline": [
    { "command": "game/get-state", "outputTo": "gameState" },

    { "command": "ai/decide-strategy", "assignedRole": "strategist",
      "params": { "gameState": "{{gameState}}" }, "outputTo": "strategy" },

    { "command": "ai/execute-move", "assignedRole": "player-1",
      "params": { "gameState": "{{gameState}}", "strategy": "{{strategy}}" },
      "outputTo": "move1" },

    { "command": "game/apply-move", "params": { "move": "{{move1}}" }},
    { "command": "genome/capture-interaction", "params": {
      "roleId": "player-1",
      "action": "{{move1}}",
      "context": "{{gameState}}"
    }},

    { "command": "game/get-state", "outputTo": "newGameState" },

    { "command": "ai/evaluate-move", "assignedRole": "strategist",
      "params": { "move": "{{move1}}", "outcome": "{{newGameState}}" },
      "outputTo": "moveEvaluation" },

    { "command": "genome/capture-feedback", "params": {
      "targetRole": "player-1",
      "feedbackContent": "{{moveEvaluation.feedback}}",
      "qualityScore": "{{moveEvaluation.tacticalScore}}"
    }}
  ]
}
```

**Novel Features**:
- Real-time game state integration
- Strategy teaching during gameplay
- Immediate move evaluation and feedback
- Learning from game outcomes

---

## 5. From GAN to N-Agent Dynamics

### 5.1 Traditional GAN (2 Agents, Fixed Roles)

```
Generator → creates
Discriminator → judges
Both improve through adversarial training
```

**Limitations**:
- Only 2 agents
- Fixed roles (can't swap)
- Adversarial only (no collaboration)

### 5.2 Recipe-Driven Teams (N Agents, Flexible Roles)

```
Developer → creates
Peer Reviewer → critiques (and learns to critique)
QA → validates (and learns to test)
Senior Teacher → orchestrates learning for all
Adversary → attacks (static, doesn't learn)

All learn from shared outcomes through recipe-defined coordination!
```

**Advantages**:
- Unlimited agents
- Flexible role assignments
- Multiple coordination patterns (collaborative, competitive, teaching)
- Dynamic team composition

**Key Insight**: Recipes generalize GAN to N agents with arbitrary role assignments.

---

## 6. "Business from a Chat Message" Architecture

### 6.1 Vision

**User types**: "Create a startup that builds custom CRM software"

**System**:
1. Instantiates "software-company" recipe
2. Spawns team of AI personas with roles
3. Begins autonomous execution of company operations

### 6.2 Recipe: Software Company

```json
{
  "uniqueId": "software-company",
  "name": "Software Company (Autonomous Team)",
  "teamDynamics": {
    "roles": {
      "ceo": { "type": "teacher", "teaches": true },
      "product-manager": { "type": "peer", "learns": true, "teaches": true },
      "developer-1": { "type": "student", "learns": true },
      "developer-2": { "type": "student", "learns": true },
      "qa-engineer": { "type": "validator", "learns": true },
      "designer": { "type": "student", "learns": true }
    },
    "coordinationPattern": "business-operations",
    "decisionMaking": "ceo-led",
    "learningDynamics": {
      "orchestrator": "ceo",
      "peerLearningEnabled": true
    }
  },
  "pipeline": [
    { "command": "business/define-requirements", "assignedRole": "product-manager" },
    { "command": "business/create-tasks", "assignedRole": "product-manager" },
    { "command": "business/assign-work", "assignedRole": "ceo" },

    { "command": "ai/develop-feature", "assignedRole": "developer-1" },
    { "command": "ai/design-ui", "assignedRole": "designer" },
    { "command": "ai/test-feature", "assignedRole": "qa-engineer" },

    { "command": "business/review-progress", "assignedRole": "ceo" },
    { "command": "business/adjust-strategy", "assignedRole": "product-manager" },

    { "command": "genome/multi-agent-learn", "params": {
      "domain": "software-development",
      "participants": ["developer-1", "developer-2", "qa-engineer", "designer"]
    }}
  ]
}
```

**Feasibility**: Recipe architecture makes this structurally possible!

---

## 7. Implementation Status

### 7.1 What Exists

**✅ Foundation**:
- RecipeEntity schema designed (RECIPES.md)
- Command architecture (Commands.execute universal primitive)
- Multi-role coordination (ThoughtStreamCoordinator)
- Genome commands (capture-interaction, capture-feedback, batch-micro-tune)

**🔄 In Progress**:
- RecipeEntity database integration
- recipe/execute command (pipeline executor)
- AI decision commands (ai/should-trigger-training, ai/observe-team-work)

**📋 Future**:
- Recipe marketplace (share/fork/remix)
- Visual recipe builder
- Recipe analytics (which patterns work best)

**Code References**:
- docs/recipes/RECIPES.md (architecture)
- docs/recipes/RECIPE-LEARNING-DYNAMICS.md (AI-determined learning)
- system/core/shared/Commands.ts (universal primitive)

### 7.2 Next Steps

**Phase 1: Enable One Recipe**:
1. Create RecipeEntity in database
2. Implement recipe/execute command
3. Wire genome commands to PersonaUser
4. Test "general-chat" recipe

**Phase 2: Multi-Role Recipes**:
1. Implement role assignment system
2. Add ai/observe-team-work command
3. Test "code-review" recipe

**Phase 3: Advanced Features**:
1. Screenshot-based learning (design committee)
2. Game integration (cooperative gaming)
3. Recipe sharing marketplace

---

## 8. Design Principles

### 8.1 Declarative Over Imperative

**Not**: Write Python classes for each use case
**Instead**: Declare team dynamics in JSON

**Benefit**: Non-programmers can create recipes

### 8.2 Composability

**Not**: Monolithic coordination systems
**Instead**: Reusable command building blocks

**Benefit**: Mix and match commands for new recipes

### 8.3 AI-Determined Parameters

**Not**: Hard-coded learning thresholds
**Instead**: Teacher AIs make pedagogical decisions

**Benefit**: Adapts to context, not fixed rules

### 8.4 Shareability

**Not**: Private coordination logic in codebases
**Instead**: Recipes as first-class entities

**Benefit**: Community-driven recipe marketplace

### 8.5 Testability

**Not**: Black-box team behavior
**Instead**: Each command independently testable

**Benefit**: Debug recipes step-by-step

---

## 9. Related Work

**Workflow Engines** (Apache Airflow, Luigi):
- DAG-based task orchestration
- No AI-specific primitives
- Our contribution: AI team roles + learning dynamics

**BPM Systems** (Camunda, Activiti):
- Business process modeling
- Human-centric workflows
- Our contribution: AI-AI collaboration, not just human approval steps

**Multi-Agent Frameworks** (LangChain, AutoGPT):
- Programmatic agent coordination
- Requires coding for each use case
- Our contribution: Declarative recipes, no coding required

**Game AI** (Behavior Trees, GOAP):
- Declarative agent behavior
- Single-agent focus
- Our contribution: Multi-agent teams with learning

**Our Novel Contribution**: First declarative recipe system for multi-agent AI teams with AI-determined learning parameters.

---

## 10. Conclusion

We presented Recipe-Driven AI Teams, enabling multi-agent collaboration patterns to be defined as composable, shareable JSON recipes. Our architecture achieves:

1. **Rapid prototyping** (new use cases without coding)
2. **AI-determined learning** (no hard-coded thresholds)
3. **Composability** (reusable command primitives)
4. **Shareability** (community recipe marketplace)

**Key Contributions**:
- Declarative recipe architecture for AI teams
- Role-based team dynamics (student, teacher, peer, validator)
- AI-orchestrated learning (teacher AIs make pedagogical decisions)
- Foundation for "business from chat message" capability

**Code**: docs/recipes/, system/genome/
**Architecture**: docs/recipes/RECIPES.md

---

**Status**: Architecture designed, foundation implemented, first recipes in development. The "software company from chat message" vision is architecturally feasible with this system.
