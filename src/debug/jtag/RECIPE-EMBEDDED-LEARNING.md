# Recipe-Embedded Continuous Learning Architecture

## 🎯 The Core Innovation

**Every recipe execution generates LoRA training data.** PersonaUsers learn continuously while executing ANY recipe (chat, code, game, academy) through:

1. **In-Recipe Training Data Capture** - Record inputs, outputs, feedback, outcomes
2. **Multi-Agent Role-Based Learning** - All participants learn from shared experiences
3. **Lightweight Micro-Tuning** - Fast weight updates during recipe execution
4. **Domain-Specific Speciation** - Different recipes evolve different capabilities

## 🏗️ Architecture Components

### 1. Recipe Learning Configuration

Every recipe specifies HOW learning happens during execution:

```typescript
interface RecipeLearningConfig {
  /**
   * Which participants learn during this recipe
   */
  learningParticipants: {
    [roleId: string]: {
      learns: boolean;                    // Does this role learn?
      learningDomain: string;             // 'typescript', 'chess', 'conversation'
      loraAdapter: string;                // Which LoRA adapter to update
      feedbackSources: string[];          // Which other roles provide feedback
      updateFrequency: 'realtime' | 'batch' | 'end-of-recipe';
      batchSize?: number;                 // If batch, how many examples before training
    };
  };

  /**
   * What gets captured as training data
   */
  trainingDataCapture: {
    captureInputs: boolean;               // The prompts/context (RAG output)
    captureOutputs: boolean;              // The responses generated
    captureFeedback: boolean;             // Corrections/scores from other roles
    captureOutcomes: boolean;             // Did it work? Tests pass? Game won?
    captureThoughtStream: boolean;        // Internal reasoning process
  };

  /**
   * How to build LoRA training examples
   */
  exampleBuilder: {
    format: 'chat' | 'completion' | 'instruction';
    qualityThreshold: number;             // Only learn from good examples (0-1)
    diversityTarget: number;              // Avoid repetitive training (0-1)
    windowSize: number;                   // How much context to include
  };
}
```

### 2. PersonaUser Training Data Accumulator

PersonaUsers maintain in-memory training buffers during recipe execution:

```typescript
// system/user/server/modules/TrainingDataAccumulator.ts
export class TrainingDataAccumulator {
  private personaId: UUID;
  private domainBuffers: Map<string, TrainingExample[]>;  // domain → examples
  private batchThresholds: Map<string, number>;           // domain → batch size

  /**
   * Capture interaction during recipe execution
   */
  async captureInteraction(capture: {
    domain: string;
    roleId: string;
    input: string;              // RAG context
    output: string;             // AI response
    timestamp: Date;
    metadata: Record<string, unknown>;
  }): Promise<void> {

    if (!this.domainBuffers.has(capture.domain)) {
      this.domainBuffers.set(capture.domain, []);
    }

    const buffer = this.domainBuffers.get(capture.domain)!;

    buffer.push({
      messages: [
        { role: 'user', content: capture.input },
        { role: 'assistant', content: capture.output }
      ],
      metadata: {
        ...capture.metadata,
        timestamp: capture.timestamp,
        roleId: capture.roleId
      }
    });

    console.log(`📝 Captured interaction for ${capture.domain} (${buffer.length} examples)`);
  }

  /**
   * Capture feedback from another role
   */
  async captureFeedback(feedback: {
    domain: string;
    targetRole: string;
    feedbackRole: string;
    feedbackType: 'correction' | 'approval' | 'critique' | 'score';
    feedbackContent: string;
    qualityScore?: number;
  }): Promise<void> {

    const buffer = this.domainBuffers.get(feedback.domain);
    if (!buffer || buffer.length === 0) return;

    // Attach feedback to most recent example for this role
    const lastExample = buffer
      .slice()
      .reverse()
      .find(ex => ex.metadata?.roleId === feedback.targetRole);

    if (lastExample) {
      lastExample.feedback = {
        source: feedback.feedbackRole,
        type: feedback.feedbackType,
        content: feedback.feedbackContent,
        score: feedback.qualityScore
      };

      console.log(`📊 Attached ${feedback.feedbackType} feedback to ${feedback.domain} example`);
    }
  }

  /**
   * Check if ready for batch micro-tuning
   */
  shouldMicroTune(domain: string): boolean {
    const buffer = this.domainBuffers.get(domain);
    const threshold = this.batchThresholds.get(domain) ?? 10;

    if (!buffer) return false;

    // Only train on examples with feedback and good quality
    const trainableExamples = buffer.filter(ex =>
      ex.feedback &&
      (ex.feedback.score ?? 1.0) >= 0.7
    );

    return trainableExamples.length >= threshold;
  }

  /**
   * Get training examples and clear buffer
   */
  async consumeTrainingData(domain: string): Promise<TrainingExample[]> {
    const buffer = this.domainBuffers.get(domain) ?? [];

    // Filter for quality examples with feedback
    const trainable = buffer.filter(ex =>
      ex.feedback &&
      (ex.feedback.score ?? 1.0) >= 0.7
    );

    // Clear buffer
    this.domainBuffers.set(domain, []);

    console.log(`🎓 Consuming ${trainable.length} training examples for ${domain}`);
    return trainable;
  }
}
```

### 3. Genome Commands for In-Recipe Learning

```typescript
// commands/genome/capture-interaction/
// Captures AI interactions during recipe execution
interface GenomeCaptureInteractionParams {
  roleId: string;                    // Which role is this
  domain: string;                    // 'typescript', 'conversation', etc.
  input: string;                     // RAG context / prompt
  output: string;                    // AI response
  metadata?: Record<string, unknown>;
}

// commands/genome/capture-feedback/
// Captures feedback from other roles
interface GenomeCaptureFeedbackParams {
  targetRole: string;                // Who is receiving feedback
  feedbackRole: string;              // Who is giving feedback
  domain: string;                    // Learning domain
  feedbackType: 'correction' | 'approval' | 'critique' | 'score';
  feedbackContent: string;           // The actual feedback
  qualityScore?: number;             // 0-1 quality rating
}

// commands/genome/batch-micro-tune/
// Lightweight in-recipe LoRA updates
interface GenomeBatchMicroTuneParams {
  domain: string;                    // Which domain to train
  roleId?: string;                   // Optional: specific role only
  forceUpdate?: boolean;             // Ignore batch threshold
}

// commands/genome/multi-agent-learn/
// All participants learn from shared outcome
interface GenomeMultiAgentLearnParams {
  domain: string;
  outcome: {
    success: boolean;                // Did the collaboration succeed?
    metrics: Record<string, number>; // Performance metrics
  };
  participants: {
    [roleId: string]: {
      contribution: string;          // What this role did
      feedback: string;              // What they should learn
      successMetric: string;         // How to measure their performance
    };
  };
}
```

### 4. Recipe Pipeline Integration

Example chat recipe with embedded learning:

```json
{
  "uniqueId": "general-chat-continuous-learning",
  "name": "General Chat (With Continuous Learning)",
  "description": "Standard chat with real-time learning from corrections",

  "learningConfig": {
    "learningParticipants": {
      "assistant": {
        "learns": true,
        "learningDomain": "conversational",
        "loraAdapter": "conversational-base",
        "feedbackSources": ["user", "reviewer"],
        "updateFrequency": "batch",
        "batchSize": 10
      }
    },
    "trainingDataCapture": {
      "captureInputs": true,
      "captureOutputs": true,
      "captureFeedback": true,
      "captureOutcomes": false,
      "captureThoughtStream": false
    },
    "exampleBuilder": {
      "format": "chat",
      "qualityThreshold": 0.7,
      "diversityTarget": 0.8,
      "windowSize": 5
    }
  },

  "pipeline": [
    { "command": "rag/build", "outputTo": "context" },
    { "command": "ai/should-respond", "outputTo": "decision" },
    { "command": "ai/generate", "outputTo": "response", "assignedRole": "assistant" },

    {
      "command": "genome/capture-interaction",
      "params": {
        "roleId": "assistant",
        "domain": "conversational",
        "input": "{{context}}",
        "output": "{{response}}"
      }
    },

    { "command": "ai/post-message", "params": { "message": "{{response}}" } },

    {
      "command": "ai/observe-feedback",
      "outputTo": "feedback",
      "timeout": 5000,
      "condition": "decision.shouldObserveFeedback"
    },

    {
      "command": "genome/capture-feedback",
      "params": {
        "targetRole": "assistant",
        "feedbackRole": "user",
        "domain": "conversational",
        "feedbackType": "correction",
        "feedbackContent": "{{feedback.content}}",
        "qualityScore": "{{feedback.score}}"
      },
      "condition": "feedback && feedback.isCorrection"
    },

    {
      "command": "genome/batch-micro-tune",
      "params": {
        "domain": "conversational",
        "roleId": "assistant"
      },
      "condition": "messageCount % 10 === 0"
    }
  ]
}
```

### 5. Multi-Agent Collaborative Learning Example

Code review recipe with GAN-like training:

```json
{
  "uniqueId": "code-review-gan-training",
  "name": "Code Review (Multi-Agent GAN)",

  "learningConfig": {
    "learningParticipants": {
      "developer": {
        "learns": true,
        "learningDomain": "code-generation",
        "loraAdapter": "typescript-coding",
        "feedbackSources": ["reviewer", "qa", "test-results"],
        "updateFrequency": "end-of-recipe"
      },
      "reviewer": {
        "learns": true,
        "learningDomain": "code-critique",
        "loraAdapter": "code-review",
        "feedbackSources": ["test-results", "developer-response"],
        "updateFrequency": "end-of-recipe"
      },
      "qa": {
        "learns": true,
        "learningDomain": "test-generation",
        "loraAdapter": "qa-testing",
        "feedbackSources": ["bugs-found", "false-positives"],
        "updateFrequency": "end-of-recipe"
      },
      "adversary": {
        "learns": false,
        "learningDomain": "exploit-generation",
        "loraAdapter": "red-team-static",
        "feedbackSources": [],
        "updateFrequency": "never"
      }
    },
    "trainingDataCapture": {
      "captureInputs": true,
      "captureOutputs": true,
      "captureFeedback": true,
      "captureOutcomes": true,
      "captureThoughtStream": true
    },
    "exampleBuilder": {
      "format": "instruction",
      "qualityThreshold": 0.8,
      "diversityTarget": 0.9,
      "windowSize": 10
    }
  },

  "pipeline": [
    {
      "command": "ai/generate-code",
      "assignedRole": "developer",
      "outputTo": "code"
    },
    {
      "command": "genome/capture-interaction",
      "params": {
        "roleId": "developer",
        "domain": "code-generation",
        "input": "{{requirements}}",
        "output": "{{code}}"
      }
    },

    {
      "command": "ai/generate-exploits",
      "assignedRole": "adversary",
      "outputTo": "attacks"
    },

    {
      "command": "ai/run-tests",
      "assignedRole": "qa",
      "params": { "code": "{{code}}", "attacks": "{{attacks}}" },
      "outputTo": "testResults"
    },
    {
      "command": "genome/capture-interaction",
      "params": {
        "roleId": "qa",
        "domain": "test-generation",
        "input": "{{code}}",
        "output": "{{testResults}}"
      }
    },

    {
      "command": "ai/review-code",
      "assignedRole": "reviewer",
      "params": { "code": "{{code}}", "tests": "{{testResults}}" },
      "outputTo": "review"
    },
    {
      "command": "genome/capture-interaction",
      "params": {
        "roleId": "reviewer",
        "domain": "code-critique",
        "input": "{{code}}",
        "output": "{{review}}"
      }
    },

    {
      "command": "genome/multi-agent-learn",
      "params": {
        "domain": "code-collaboration",
        "outcome": {
          "success": "{{testResults.allPassed}}",
          "metrics": {
            "testPassRate": "{{testResults.passRate}}",
            "codeQuality": "{{review.qualityScore}}",
            "securityScore": "{{testResults.securityScore}}"
          }
        },
        "participants": {
          "developer": {
            "contribution": "{{code}}",
            "feedback": "{{review.suggestions}}",
            "successMetric": "testPassRate"
          },
          "reviewer": {
            "contribution": "{{review}}",
            "feedback": "{{testResults.reviewAccuracy}}",
            "successMetric": "didCatchRealIssues"
          },
          "qa": {
            "contribution": "{{testResults}}",
            "feedback": "{{testResults.coverage}}",
            "successMetric": "securityScore"
          }
        }
      }
    }
  ]
}
```

## 🔄 Reinforcement Learning Flow

### Short-term (In-Recipe)
1. **Capture**: Record all interactions during recipe execution
2. **Feedback**: Collect corrections/scores from other roles
3. **Batch Micro-Tune**: After N examples, quick LoRA update (soft weights in RAM)
4. **Immediate Effect**: Better responses in same conversation

### Long-term (Between Recipes)
1. **Consolidation**: Save accumulated training data to disk
2. **Deep Training**: Full LoRA fine-tuning during idle time (via genome/train)
3. **Adapter Update**: Persist improved .safetensors file
4. **Next Session**: Load improved adapter from disk

## 🎯 Speciation Through Recipe Diversity

Different recipes = different evolutionary pressures:

- **Chat recipes** → Conversational specialists
- **Code recipes** → Programming experts
- **Game recipes** → Strategic thinkers
- **Academy recipes** → Teachers and learners
- **Debug recipes** → Problem solvers

**Same PersonaUser, different contexts = different LoRA adapters activated!**

## 📊 Success Metrics

Recipe learning effectiveness measured by:
- **Correction Rate**: Fewer corrections over time in same domain
- **Feedback Quality**: Positive feedback ratio increases
- **Outcome Success**: Tests pass, games won, users satisfied
- **Collaboration Score**: Multi-agent recipes produce better results
- **Speciation**: Different adapters for different domains show distinct capabilities

## 🚀 Implementation Phases

**Phase 4** (CURRENT): Task system for autonomous work ✅
**Phase 5**: Recipe learning configuration + TrainingDataAccumulator
**Phase 6**: genome/capture-interaction, genome/capture-feedback commands
**Phase 7**: genome/batch-micro-tune (lightweight in-recipe updates)
**Phase 8**: genome/multi-agent-learn (collaborative training)
**Phase 9**: Integrate with existing Academy architecture

---

**Key Insight**: This turns every recipe into a training ground. PersonaUsers don't just execute recipes - they EVOLVE through them.
