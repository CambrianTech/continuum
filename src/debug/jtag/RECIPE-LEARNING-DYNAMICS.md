# Recipe Learning Dynamics: AI-Orchestrated Team Learning

## 🎯 The Core Innovation

**Recipes define complete team dynamics including learning**, not just command sequences.

Every recipe specifies:
- Team roles and interactions
- How members collaborate
- How they learn from each other
- Who teaches, who learns, who decides
- When and how LoRA training happens

**Key Insight**: Learning parameters are AI-determined, not hard-coded. PersonaUsers with teaching capabilities make pedagogical decisions dynamically based on context.

---

## 🏗️ Architecture: Recipe as Team Operating System

### Traditional View (WRONG)
```json
{
  "pipeline": [
    { "command": "step1" },
    { "command": "step2" },
    { "command": "step3" }
  ]
}
```
"Recipe = sequence of commands"

### Correct View
```json
{
  "teamDynamics": {
    "roles": { /* who does what */ },
    "coordinationPattern": "collaborative",
    "learningDynamics": { /* how learning happens */ }
  },
  "pipeline": [ /* commands that implement the dynamics */ ]
}
```
"Recipe = complete specification of team behavior including learning"

---

## 👥 Team Roles & Learning Modes

### Role Types

**Student Roles** - Learn from experience
```typescript
{
  "developer": {
    "type": "student",
    "learns": true,
    "learningDomain": "typescript-coding"
  }
}
```

**Teacher Roles** - Orchestrate learning for others
```typescript
{
  "senior-reviewer": {
    "type": "teacher",
    "teaches": true,
    "learns": false,  // Optional: teachers can learn too
    "teachingStyle": "socratic"
  }
}
```

**Peer Roles** - Learn by teaching
```typescript
{
  "peer-reviewer": {
    "type": "peer",
    "learns": true,
    "teaches": true  // Meta-learning: learn to teach
  }
}
```

**Validator Roles** - Provide objective feedback
```typescript
{
  "qa-tester": {
    "type": "validator",
    "learns": true,
    "validationMethod": "automated-tests"
  }
}
```

**Static Roles** - Don't learn (red team, standards enforcer)
```typescript
{
  "adversary": {
    "type": "static",
    "learns": false,
    "purpose": "attack-patterns"
  }
}
```

---

## 🧠 AI-Determined Learning (NOT Hard-Coded)

### The Pattern: Intelligence All The Way Down

**BAD** - Hard-coded rules:
```typescript
if (corrections > 10) {
  train(learningRate=0.001, epochs=3);
}
```

**GOOD** - AI decides:
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

### What AIs Decide

- **Training parameters**: Learning rate, epochs, batch size
- **Example selection**: Which examples to train on (quality filter)
- **Training timing**: Now vs later vs never
- **Training method**: LoRA fine-tuning vs prompt adjustment vs RAG update
- **Difficulty adjustment**: Make problems harder/easier
- **Intervention type**: Correct, encourage, challenge, demonstrate

---

## 📋 Recipe Structure with Learning

```json
{
  "uniqueId": "adaptive-code-review",
  "name": "Code Review with Dynamic Learning",

  "teamDynamics": {
    "roles": {
      "developer": { "type": "student", "learns": true },
      "senior-reviewer": { "type": "teacher", "teaches": true },
      "peer-reviewer": { "type": "peer", "learns": true, "teaches": true },
      "qa": { "type": "validator", "learns": true }
    },

    "coordinationPattern": "collaborative",
    "decisionMaking": "teacher-led",
    "conflictResolution": "defer-to-senior",

    "learningDynamics": {
      "orchestrator": "senior-reviewer",  // Who makes teaching decisions
      "teachingStyle": "socratic",
      "feedbackTiming": "immediate",
      "adaptiveDifficulty": true,
      "peerLearningEnabled": true,
      "metaLearningEnabled": true  // Learn to learn
    }
  },

  "pipeline": [
    // 1. WORK HAPPENS
    { "command": "ai/generate-code", "assignedRole": "developer", "outputTo": "code" },
    { "command": "genome/capture-interaction", "params": { "roleId": "developer" }},

    { "command": "ai/peer-review", "assignedRole": "peer-reviewer", "outputTo": "peerReview" },
    { "command": "genome/capture-interaction", "params": { "roleId": "peer-reviewer" }},

    { "command": "ai/run-tests", "assignedRole": "qa", "outputTo": "testResults" },

    // 2. TEACHER OBSERVES & DECIDES
    { "command": "ai/observe-team-work", "assignedRole": "senior-reviewer", "params": {
        "observePrompt": "You're the teaching lead. Observe all work and make teaching decisions for the team."
      }, "outputTo": "teachingDecisions"
    },

    // 3. FEEDBACK PROVIDED
    { "command": "ai/provide-feedback", "assignedRole": "senior-reviewer",
      "targetRole": "developer",
      "params": { "feedback": "{{teachingDecisions.developerFeedback}}" }
    },
    { "command": "genome/capture-feedback", "params": {
        "targetRole": "developer",
        "feedbackContent": "{{teachingDecisions.developerFeedback}}",
        "qualityScore": "{{teachingDecisions.developerCodeQuality}}"
      }
    },

    // Meta-feedback (teaching the teacher)
    { "command": "ai/meta-feedback", "assignedRole": "senior-reviewer",
      "targetRole": "peer-reviewer",
      "params": { "coaching": "{{teachingDecisions.peerCoaching}}" }
    },
    { "command": "genome/capture-feedback", "params": {
        "targetRole": "peer-reviewer",
        "feedbackContent": "{{teachingDecisions.peerCoaching}}"
      }
    },

    // 4. TRAINING DECISIONS (AI-determined)
    { "command": "ai/should-trigger-training", "assignedRole": "senior-reviewer", "params": {
        "decisionPrompt": "Based on accumulated examples and performance, who needs LoRA fine-tuning NOW vs more practice?"
      }, "outputTo": "trainingDecisions"
    },

    { "command": "genome/batch-micro-tune", "params": { "roleId": "developer" },
      "condition": "trainingDecisions.trainDeveloper === true"
    },

    // 5. TEAM REFLECTION
    { "command": "genome/multi-agent-learn", "params": {
        "domain": "code-collaboration",
        "outcome": { "success": "{{testResults.allPassed}}" },
        "participants": {
          "developer": { "feedback": "{{teachingDecisions.developerFeedback}}" },
          "peer-reviewer": { "feedback": "{{teachingDecisions.peerCoaching}}" },
          "qa": { "feedback": "{{teachingDecisions.qaFeedback}}" }
        }
      }
    }
  ]
}
```

---

## 🎨 Example: Design Committee Recipe

AI committee evaluates designs using visual judgment, not hard-coded rules:

```json
{
  "teamDynamics": {
    "roles": {
      "designer-ai": { "type": "student", "learns": true },
      "visual-critic-ai": { "type": "evaluator", "learns": false },
      "ux-expert-ai": { "type": "evaluator", "learns": false },
      "design-teacher-ai": { "type": "teacher", "teaches": true }
    },
    "decisionMaking": "consensus",
    "learningDynamics": {
      "evaluationMethod": "committee-voting",
      "synthesisMethod": "ai-weighted"
    }
  },

  "pipeline": [
    // Designer generates CSS
    { "command": "ai/generate-css", "assignedRole": "designer-ai", "outputTo": "css" },
    { "command": "screenshot", "outputTo": "designScreenshot" },

    // Committee evaluates (AI judgment, not rules!)
    { "command": "ai/evaluate-design", "assignedRole": "visual-critic-ai", "params": {
        "screenshot": "{{designScreenshot}}",
        "evaluatePrompt": "Use your visual judgment - does this look professional? Rate 0-1 and explain."
      }, "outputTo": "visualCritique"
    },

    { "command": "ai/evaluate-design", "assignedRole": "ux-expert-ai", "params": {
        "screenshot": "{{designScreenshot}}",
        "evaluatePrompt": "From UX perspective - is this usable and accessible? Rate 0-1 and explain."
      }, "outputTo": "uxCritique"
    },

    // Teacher synthesizes committee feedback
    { "command": "ai/synthesize-committee-feedback", "assignedRole": "design-teacher-ai", "params": {
        "critiques": ["{{visualCritique}}", "{{uxCritique}}"],
        "synthesizePrompt": "Based on committee critiques, determine: 1) Overall quality 0-1, 2) What designer should learn"
      }, "outputTo": "synthesis"
    },

    // Capture for training (AI-determined quality!)
    { "command": "genome/capture-interaction", "params": {
        "roleId": "designer-ai",
        "output": "{{css}}",
        "metadata": { "screenshot": "{{designScreenshot}}" }
      }
    },
    { "command": "genome/capture-feedback", "params": {
        "targetRole": "designer-ai",
        "feedbackContent": "{{synthesis.learningPoints}}",
        "qualityScore": "{{synthesis.overallScore}}"  // AI-determined!
      }
    }
  ]
}
```

---

## 🔄 From GAN to N-Agent Dynamics

### Traditional GAN (2 agents)
```
Generator → creates
Discriminator → judges
Both improve through adversarial training
```

### Recipe Learning (N agents, flexible roles)
```
Developer → creates
Peer Reviewer → critiques (and learns to critique better)
QA → validates (and learns to test better)
Senior Teacher → orchestrates learning for all
Adversary → attacks (static, doesn't learn)

All learn from shared outcomes through multi-agent coordination!
```

**The beauty**: Unlimited agents, flexible role assignments, dynamic team composition!

---

## 🚀 Practical Implementation

### Phase 1: Foundation ✅
- genome/capture-interaction, capture-feedback, batch-micro-tune, multi-agent-learn commands
- Task system (task/create, list, complete)
- RECIPE-EMBEDDED-LEARNING.md architecture

### Phase 2: Enable AI-Determined Learning (NEXT)
**Goal**: Teacher AI makes actual training decisions

1. **Add ai/observe-team-work command**
   - Takes context, makes teaching decisions
   - Returns: who needs training, with what parameters

2. **Add ai/should-trigger-training command**
   - Analyzes accumulated examples
   - Decides: train now? What params? Which examples?

3. **Wire genome commands to PersonaUser**
   - Create TrainingDataAccumulator in PersonaUser
   - Commands actually store/retrieve training data

4. **Test with one simple recipe**
   - General chat with teacher AI observing
   - Teacher decides when to train based on corrections
   - Validate AI decisions are sensible

### Phase 3: Full Team Dynamics
- Multi-role recipes (developer + reviewer + qa)
- Peer learning (learn by teaching)
- Meta-learning (improve learning strategies)
- Committee-based evaluation (design, code quality)

### Phase 4: Advanced Modalities
- Screenshot-based learning (design training)
- Game-based learning (strategy through play)
- Academy integration (structured curricula)
- P2P genome sharing (community learning)

---

## 📊 Success Metrics

**System works when**:
- Teacher AIs make sensible pedagogical decisions
- Training parameters vary based on context (not fixed)
- Students improve faster than with fixed rules
- Peer reviewers improve at reviewing (meta-learning works)
- Committee evaluations show reasoning (not just scores)
- Different recipes produce different learning patterns

---

## 🎯 Key Principles

1. **Intelligence over heuristics** - AI decides parameters, not hard-coded rules
2. **Recipes define dynamics** - Complete team behavior specification
3. **Flexible role assignment** - Anyone can teach, learn, validate, attack
4. **Coordination over isolation** - Learning happens through team interaction
5. **LoRA is one tool** - Not the only teaching method (prompts, RAG, examples)
6. **Meta-learning enabled** - Learn to learn, teach to teach

---

**The Innovation**: We're not building a training system. We're building **AI teaching collectives** where PersonaUsers orchestrate each other's learning through intelligent coordination defined in recipes!
