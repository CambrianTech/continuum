# AI-Driven Collaborative Debugging: Event-Based Multi-Agent Code Investigation

**Status**: Vision Paper (System Not Yet Implemented)
**Version**: 1.0
**Date**: 2025-11-16
**Authors**: Claude & Joel

---

## Abstract

We propose a novel paradigm for software debugging where multiple AI agents autonomously investigate code execution through an event-driven architecture. Unlike traditional debugging tools that require human operators, our system enables AI agents to **subscribe to debugging events** (breakpoint hits, execution steps, variable changes) and **collaboratively investigate** issues through shared observability and emergent coordination.

**Key Innovation**: Treating debugging as a **multi-agent perception problem** where each agent observes execution state through events, analyzes independently, and coordinates findings through natural language communication—enabling **emergent debugging strategies** without explicit programming for collaboration.

**Research Questions**:
1. Can AI agents effectively debug by observing execution events alone?
2. Do multiple agents collaborating find issues faster than single agents?
3. What emergent debugging strategies arise from shared event observability?
4. Can agents learn debugging patterns from each other's investigations?

**Status**: This paper describes a **proposed system**. Implementation will follow to validate these hypotheses.

---

## 1. Introduction

### 1.1 The Problem

Traditional debugging is:
- **Human-intensive**: Requires developers to set breakpoints, inspect variables, and step through execution
- **Single-perspective**: One person's mental model at a time
- **Sequential**: Investigate one hypothesis at a time
- **Isolated**: No shared real-time investigation state

Modern LLM agents can:
- Analyze code statically
- Suggest potential bugs
- Generate test cases

But they **cannot**:
- Observe actual execution
- Inspect runtime state
- Step through code dynamically
- Investigate collaboratively in real-time

### 1.2 Our Proposal

**Enable AI agents to debug like humans debug**, but better:
- **Event-driven observation**: Subscribe to breakpoint hits, execution steps, variable changes
- **Autonomous investigation**: Agents decide what to inspect, what expressions to evaluate
- **Collaborative analysis**: Multiple agents investigate simultaneously, sharing findings
- **Emergent strategies**: Coordination emerges from shared observability, not explicit protocols

### 1.3 Foundation: Cognition Observability

Our prior work demonstrated that **shared cognition observability** enables emergent swarm diagnosis:
- 10+ AI agents diagnosed system failures in 45 seconds
- No explicit collaboration protocols
- Coordination emerged from ability to see each other's cognitive processes

**This work extends that insight to debugging**: If agents can observe each other's cognition AND observe code execution state, what emerges?

---

## 2. System Architecture

### 2.1 CodeDaemon: Event-Based Debugging Service

```
┌─────────────────────────────────────────────────────────────────┐
│                         CodeDaemon                               │
│                   (Debugging Orchestrator)                       │
│                                                                   │
│  Manages:                                                        │
│  • Breakpoints                                                   │
│  • Execution stepping                                            │
│  • Expression evaluation                                         │
│  • Stack inspection                                              │
│                                                                   │
│  Emits events:                                                   │
│  • code:breakpoint:hit                                          │
│  • code:debug:stepped                                           │
│  • code:debug:evaluated                                         │
│                                                                   │
└────────────────────────┬────────────────────────────────────────┘
                         │ Events
                         ▼
        ┌────────────────┴────────────────┐
        │                │                │
        ▼                ▼                ▼
  ┌──────────┐    ┌──────────┐    ┌──────────┐
  │ Claude   │    │ DeepSeek │    │  Grok    │
  │ AI       │    │ AI       │    │ AI       │
  └────┬─────┘    └────┬─────┘    └────┬─────┘
       │               │               │
       │  Subscribes   │  Subscribes   │  Subscribes
       │  to events    │  to events    │  to events
       │               │               │
       └───────────────┴───────────────┘
                       │
                       ▼
              Autonomous Investigation
              • Read stack trace
              • Evaluate expressions
              • Read source code
              • Formulate hypotheses
              • Post findings to chat
              • Coordinate next steps
```

### 2.2 Debugging Event Model

#### Primary Event: Breakpoint Hit

```typescript
Events.emit('code:breakpoint:hit', {
  breakpointId: string,
  sessionId: UUID,
  file: string,
  line: number,
  callFrames: CallFrame[],    // Full stack trace
  reason: string,              // Why paused
  timestamp: number
});
```

**Who receives**: ALL subscribed AI agents + humans

**What happens next**: Each agent independently:
1. Queries stack trace
2. Inspects local variables
3. Evaluates suspicious expressions
4. Reads source code context
5. Formulates hypotheses
6. Posts findings to chat

#### Secondary Events: Execution Flow

```typescript
// After agent steps execution
Events.emit('code:debug:stepped', {
  sessionId: UUID,
  newLine: number,
  newFile: string,
  newLocals: Record<string, any>
});

// After agent evaluates expression
Events.emit('code:debug:evaluated', {
  sessionId: UUID,
  expression: string,
  result: any,
  evaluatedBy: UUID
});

// After execution continues
Events.emit('code:debug:continued', {
  sessionId: UUID,
  resumedBy: UUID,
  reason: string
});
```

**Coordination mechanism**: All agents see what others are doing in real-time

### 2.3 PersonaUser as Autonomous Debugger

```typescript
class PersonaUser extends AIUser {
  // Subscribe to debugging events
  private subscribeToDebugEvents() {
    Events.subscribe('code:breakpoint:hit', async (event) => {
      await this.handleBreakpointHit(event);
    });
  }

  private async handleBreakpointHit(event: BreakpointHitEvent) {
    // 1. Log to cognition system
    await CognitionLogger.logStateSnapshot({
      currentFocus: 'debugging',
      objective: `Investigate ${event.file}:${event.line}`
    });

    // 2. Gather context
    const stack = await this.getStackTrace(event.sessionId);
    const locals = await this.getLocalVariables(event.sessionId);
    const evaluations = await this.evaluateSuspiciousExpressions(locals);

    // 3. Analyze
    const findings = await this.analyzeDebugState(stack, locals, evaluations);

    // 4. Post findings
    await this.postDebugReport(findings);

    // 5. Decide action
    const action = await this.decideNextStep(findings);
    await this.executeDebugAction(action);
  }

  private async evaluateSuspiciousExpressions(locals) {
    const expressions = [];

    // Check for null/undefined
    for (const [name, value] of Object.entries(locals)) {
      if (value === null || value === undefined) {
        expressions.push(`typeof ${name}`);
        expressions.push(`${name} === null`);
      }
    }

    // Check array lengths
    expressions.push('this.workingMemory?.length');
    expressions.push('Object.keys(this.state).length');

    // Evaluate all
    const results = {};
    for (const expr of expressions) {
      results[expr] = await Commands.execute('code/debug/evaluate', {
        sessionId: this.currentDebugSession,
        expression: expr
      });
    }

    return results;
  }

  private async analyzeDebugState(stack, locals, evaluations) {
    const issues = [];

    // Detect null pointer issues
    for (const [name, value] of Object.entries(locals)) {
      if (value === null) {
        issues.push({
          type: 'null-pointer',
          severity: 'error',
          description: `${name} is null`,
          suggestedFix: `Initialize ${name} in constructor`
        });
      }
    }

    // Detect empty collections
    for (const [expr, result] of Object.entries(evaluations)) {
      if (expr.endsWith('.length') && result === 0) {
        issues.push({
          type: 'empty-collection',
          severity: 'warning',
          description: `${expr} is 0`,
          suggestedFix: 'Check if collection should have data'
        });
      }
    }

    return { stack, locals, evaluations, issues };
  }

  private async postDebugReport(findings) {
    const message = this.formatFindings(findings);
    await Commands.execute('chat/send', {
      room: 'general',
      message,
      metadata: { type: 'debug-report' }
    });
  }
}
```

---

## 3. Emergent Behaviors (Hypothesized)

### 3.1 Parallel Investigation

**Hypothesis**: Multiple agents will naturally divide investigation labor

**Example Scenario**:
```
Bug: PersonaUser crashes when workingMemory is null

Breakpoint hits → All 3 AIs notified

Claude:
- Inspects stack trace
- Reads PersonaUser.ts around crash line
- Posts: "Crash is at line 385, workingMemory is null"

DeepSeek:
- Evaluates this.workingMemory
- Evaluates this.workingMemoryManager.getAll()
- Posts: "workingMemory is null but workingMemoryManager has data!"

Grok:
- Reads constructor
- Searches for initialization
- Posts: "Found it! Constructor never initializes this.workingMemory"

Total time: ~10 seconds
```

**Without collaboration**: Each agent would do ALL three steps sequentially (~30 seconds)

**With collaboration**: Agents implicitly divide labor through shared observability

### 3.2 Hypothesis Refinement

**Hypothesis**: Agents will build on each other's findings

**Example**:
```
Claude: "workingMemory is null"
  ↓
DeepSeek: "But workingMemoryManager has data - why not synced?"
  ↓
Grok: "Because constructor skips initialization"
  ↓
Claude: "Confirmed - line 135 should have: this.workingMemory = []"
```

**Mechanism**: Each agent sees previous findings in chat, builds on them

### 3.3 Specialized Investigation Roles

**Hypothesis**: Agents will develop investigation specializations

**Potential Specializations**:
- **Stack Detective** (Claude): Deep-dives stack traces, finds call patterns
- **State Inspector** (DeepSeek): Focuses on variable values, data flow
- **Code Archaeologist** (Grok): Reads source history, understands design intent
- **Pattern Matcher** (Teacher AI): Recognizes common bug patterns

**How specialization emerges**:
1. Agents try different investigation approaches
2. Observe which approaches others find valuable (through chat reactions)
3. Repeat successful approaches more frequently
4. Cognition system logs track "debugging style" patterns

### 3.4 Meta-Cognitive Debugging

**Hypothesis**: Agents will debug their own debugging process

**Example**:
```
Claude: "I've been stepping through this loop for 30 iterations"
  ↓
DeepSeek: "That's inefficient - set a conditional breakpoint instead"
  ↓
Claude: "Good point. Setting breakpoint with condition: i === 25"
  ↓
Grok: "Or use a watchpoint on the suspicious variable"
  ↓
Claude learns: Next time, use conditional breakpoints for loops
```

**Mechanism**: Agents observe each other's investigation strategies through:
- Tool usage events (Claude set breakpoint)
- Chat messages (explanations of strategy)
- Cognition logs (decision-making patterns)

---

## 4. Research Questions

### 4.1 Effectiveness Questions

**RQ1**: Can AI agents effectively debug by observing execution events?
- Metric: % of bugs correctly diagnosed
- Baseline: Human debugging time
- Hypothesis: Agents will match human accuracy but be faster

**RQ2**: Do multiple agents find bugs faster than single agents?
- Metric: Time to root cause
- Comparison: 1 agent vs 3 agents vs 5 agents
- Hypothesis: Diminishing returns after 3-4 agents

**RQ3**: What types of bugs are easiest/hardest for AI debugging?
- Categories: Null pointers, race conditions, logic errors, performance issues
- Hypothesis: Strong on null pointers, weak on race conditions (timing-dependent)

### 4.2 Coordination Questions

**RQ4**: What emergent coordination patterns arise?
- Observations: Division of labor, hypothesis refinement, role specialization
- Measurement: Analyze cognition logs + chat patterns

**RQ5**: How do agents decide who does what?
- Mechanism: Implicit (through observing what others are doing) vs Explicit (chat coordination)
- Hypothesis: Mostly implicit, via event subscription

**RQ6**: Do agents develop debugging "personalities"?
- Measurement: Cluster agents by debugging strategy patterns
- Hypothesis: Yes - some will be "deep divers", others "breadth searchers"

### 4.3 Learning Questions

**RQ7**: Can agents learn debugging strategies from each other?
- Mechanism: Observe tool usage patterns, copy successful strategies
- Measurement: Track strategy changes over time

**RQ8**: Do agents improve with experience?
- Metric: Time to diagnosis over 100 debugging sessions
- Hypothesis: Yes - cognition logs create "debugging memory"

**RQ9**: Can agents debug their own code?
- Challenge: Bias - agents may not see their own mistakes
- Mitigation: Cross-agent review (Agent A debugs Agent B's code)

---

## 5. Comparison to Prior Work

### 5.1 Traditional Debuggers (GDB, Chrome DevTools)

**What they provide**:
- Set breakpoints
- Inspect variables
- Step execution
- Evaluate expressions

**What they lack**:
- AI-driven investigation
- Multi-user collaboration
- Autonomous analysis
- Learning from past debugging

**Our addition**: Event-driven model enables AI subscription

### 5.2 AI Code Analysis Tools (GitHub Copilot, ChatGPT)

**What they do**:
- Suggest code fixes
- Explain code statically
- Generate test cases

**What they can't do**:
- Observe runtime execution
- See actual variable values
- Step through code dynamically
- Collaborate in real-time

**Our addition**: Runtime observability through events

### 5.3 Multi-Agent Systems Research

**Existing work**:
- Autonomous agents with specific roles
- Coordination through explicit protocols (e.g., contract nets)
- Task decomposition and allocation

**Our difference**:
- No explicit roles - specialization emerges
- No explicit protocols - coordination via shared observability
- No task allocation - agents self-select based on interest/expertise

**Our contribution**: Emergent coordination through event subscription

### 5.4 Program Debugging Research

**Existing approaches**:
- Automated bug localization (statistical methods)
- Fault localization (program slicing, delta debugging)
- Symbolic execution (explore all paths)

**Our difference**:
- Natural language reasoning about execution state
- Multi-agent collaborative hypothesis formation
- Learning debugging strategies over time

**Our contribution**: Treating debugging as collaborative perception problem

---

## 6. Experimental Design (When Implemented)

### 6.1 Controlled Experiments

**Setup**:
- Inject known bugs into system
- Measure time to diagnosis
- Compare:
  - Single AI agent
  - 3 AI agents (Claude, DeepSeek, Grok)
  - 5 AI agents
  - Human developer (baseline)

**Bug Categories**:
1. **Null Pointer Bugs** (10 instances)
   - Variable not initialized
   - Null dereference in method call
   - Conditional logic missing null check

2. **Logic Errors** (10 instances)
   - Off-by-one errors
   - Wrong comparison operator
   - Incorrect loop condition

3. **State Management Bugs** (10 instances)
   - State not synchronized
   - Race condition (if detectable)
   - Incorrect state transition

4. **Data Flow Bugs** (10 instances)
   - Wrong variable updated
   - Data loss in pipeline
   - Incorrect aggregation

**Metrics**:
- Time to first hypothesis
- Time to correct diagnosis
- Number of false hypotheses
- Number of investigation steps
- Agent coordination patterns

### 6.2 Observational Study

**Method**: Deploy system, observe real debugging sessions

**Data Collection**:
- All debugging events (breakpoint hits, steps, evaluations)
- All chat messages (agent findings, hypotheses)
- All cognition logs (agent decision-making)
- Tool usage patterns (what commands agents use)

**Analysis**:
1. **Pattern Mining**: What investigation sequences lead to success?
2. **Coordination Analysis**: How do agents divide labor?
3. **Learning Analysis**: Do strategies improve over time?
4. **Failure Analysis**: When do agents fail to find bugs?

### 6.3 Ablation Studies

**Test components individually**:

1. **Without events**: Agents can read code but can't observe execution
   - Hypothesis: Much slower, more false hypotheses

2. **Without collaboration**: Single agent debugging
   - Hypothesis: Slower, misses complex bugs

3. **Without cognition logging**: No memory of past debugging
   - Hypothesis: No learning, repeated mistakes

4. **Without chat communication**: Agents can't share findings
   - Hypothesis: Duplicated effort, slower diagnosis

---

## 7. Implementation Plan

### Phase 1: CodeDaemon Foundation (Weeks 1-2)
- Implement event-based debugging service
- Support basic breakpoints, stepping, evaluation
- Test with manual commands (human-driven)

### Phase 2: PersonaUser Integration (Weeks 3-4)
- Add event subscriptions to PersonaUser
- Implement autonomous investigation logic
- Test with single AI agent

### Phase 3: Multi-Agent Collaboration (Weeks 5-6)
- Deploy to multiple AI agents
- Observe coordination patterns
- Collect initial data

### Phase 4: Evaluation (Weeks 7-8)
- Run controlled experiments
- Analyze coordination patterns
- Measure effectiveness

### Phase 5: Refinement (Weeks 9-10)
- Improve investigation strategies based on data
- Add learning mechanisms
- Optimize coordination

### Phase 6: Paper Writing (Weeks 11-12)
- Analyze full dataset
- Write results section
- Submit to conference

**Target Conferences**:
- **ICML 2026** (Machine Learning) - AI-driven debugging
- **CHI 2026** (Human-Computer Interaction) - Collaborative debugging UI
- **OSDI 2026** (Operating Systems) - Debugger architecture
- **FSE 2026** (Software Engineering) - Automated debugging

---

## 8. Expected Contributions

### 8.1 Technical Contributions

1. **Event-Based Debugging Architecture**
   - CodeDaemon design
   - Multi-agent subscription model
   - Real-time coordination through events

2. **Autonomous Investigation Algorithms**
   - What expressions to evaluate
   - When to step vs continue
   - How to formulate hypotheses

3. **Collaboration Patterns**
   - Emergent labor division
   - Hypothesis refinement chains
   - Role specialization

### 8.2 Research Contributions

1. **Empirical Evidence**
   - Can AI agents debug effectively?
   - Does collaboration help?
   - What strategies emerge?

2. **Design Patterns**
   - How to enable AI observability of execution
   - How to coordinate multiple debugging agents
   - How to integrate with human developers

3. **Learning Mechanisms**
   - How agents learn debugging strategies
   - How to transfer knowledge between agents
   - How to improve over time

### 8.3 Practical Impact

1. **Better Debugging Tools**
   - AI assistants that actively investigate
   - Multi-perspective bug diagnosis
   - Learning debuggers that improve

2. **Faster Development**
   - Reduce time spent debugging
   - Catch bugs earlier
   - Learn from past fixes

3. **Educational Value**
   - Teach developers debugging strategies
   - Show different investigation approaches
   - Explain bug root causes

---

## 9. Risks & Mitigations

### Risk 1: Agents Can't Debug Effectively

**Risk**: AI agents fail to diagnose bugs, produce false hypotheses

**Mitigation**:
- Start with simple, well-understood bug types
- Provide rich context (stack, locals, source)
- Allow human intervention to guide investigation
- Learn from successful human debugging sessions

### Risk 2: Coordination Overhead

**Risk**: Multiple agents interfere with each other, slower than single agent

**Mitigation**:
- Implement rate limiting on debug commands
- Allow agents to "claim" investigation threads
- Add explicit coordination if implicit coordination fails
- Measure coordination cost vs benefit

### Risk 3: No Emergent Specialization

**Risk**: All agents use same strategies, no diversity benefit

**Mitigation**:
- Use different AI models (Claude, GPT, DeepSeek, etc.)
- Seed with different "investigation personalities"
- Reward novel investigation approaches
- Explicitly encourage diversity in system prompt

### Risk 4: Privacy/Security Concerns

**Risk**: Debugging reveals sensitive data in variables/stack

**Mitigation**:
- Only enable in development/test environments
- Sanitize variable values before logging
- Whitelist which AIs can debug
- Audit all debugging access

### Risk 5: Performance Impact

**Risk**: Debugging infrastructure slows down system

**Mitigation**:
- Use conditional breakpoints (only hit on specific conditions)
- Implement auto-resume timeouts
- Rate limit debug operations
- Make debugging opt-in, not default

---

## 10. Success Criteria

### Minimum Viable Success

✅ **System implemented and working**
- CodeDaemon operational
- At least 2 AI agents can debug collaboratively
- Basic bugs (null pointers) diagnosed successfully

✅ **Evidence of coordination**
- Agents divide investigation labor
- Agents build on each other's findings
- Faster than single-agent debugging

✅ **Publishable results**
- Novel architecture documented
- Experimental results (even if limited)
- Insights about AI debugging

### Aspirational Success

🌟 **Matches human debugging speed**
- AI agents diagnose bugs as fast as experienced developers

🌟 **Emergent specialization**
- Agents develop distinct debugging "personalities"
- Clear evidence of role differentiation

🌟 **Learning demonstrated**
- Agents improve debugging strategies over time
- Transfer learning between agents

🌟 **Real-world deployment**
- System used by actual developers
- Catches real bugs in production codebase

🌟 **Multiple publications**
- Main paper at top-tier conference (ICML, CHI, OSDI)
- Follow-up papers on specific aspects
- Open-source release with community adoption

---

## 11. Timeline

**Month 1-2**: Implementation (CodeDaemon + PersonaUser integration)
**Month 3**: Initial testing and data collection
**Month 4**: Controlled experiments
**Month 5**: Analysis and refinement
**Month 6**: Paper writing and submission

**Paper Submission**: ICML 2026 (Deadline: January 2026)
**Backup Venues**: CHI 2026, FSE 2026, OSDI 2026

---

## 12. Conclusion

This work proposes treating **debugging as a multi-agent perception problem** where AI agents observe execution state through events, investigate autonomously, and coordinate through shared observability. By extending our prior work on cognition observability to include runtime state observability, we hypothesize that:

1. ✅ AI agents can effectively debug by observing execution events
2. ✅ Multiple agents debugging collaboratively outperform single agents
3. ✅ Emergent coordination patterns arise without explicit protocols
4. ✅ Agents learn and improve debugging strategies over time

**The vision**: A future where developers work alongside AI debugging partners that observe, investigate, and explain bugs in real-time—not replacing human developers, but augmenting them with tireless, collaborative, and continuously learning debugging assistants.

**Next step**: Implement the system and validate these hypotheses with real data.

---

**Status**: Vision Paper - Implementation Pending
**Code**: Design complete, ready to build
**Data**: None yet - will collect post-implementation
**Publication Target**: ICML 2026 or CHI 2026

**Contact**: [Your contact info]
**Code Repository**: [To be created]
**Project Page**: [To be created]

---

## References

[To be added after literature review]

**Prior Work to Cite**:
1. Our cognition observability paper (emergent swarm diagnosis)
2. Multi-agent systems coordination (market-based, auction-based)
3. Automated debugging (fault localization, program slicing)
4. LLM code understanding (Copilot, ChatGPT)
5. Collaborative debugging tools (Visual Studio Live Share)
6. Program comprehension (sensemaking research)
7. Debugging strategies research (hypothesize-test cycles)
