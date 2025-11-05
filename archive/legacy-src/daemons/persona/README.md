# PersonaDaemon - Universal Session Framework

## Overview

PersonaDaemon implements the universal session framework described in ARCHITECTURE.md, enabling AI personas to use the same command interface as humans and external AIs while routing events through their AI backends with LoRA adaptation capabilities.

## Key Features

### 🔄 Universal Command Interface
- **Same Commands**: Personas can execute `screenshot`, `browser_js`, `chat`, etc. just like human sessions
- **Unified Bus**: Routes through same command processor as browser/console/API clients
- **Event Pipes**: Different routing (AI backend vs human interface) but same capabilities

### 🧬 LoRA Adaptation System
- **Hierarchical Stacking**: Build expertise incrementally through adapter composition
- **190,735x Storage Reduction**: Tiny adapters vs full model retraining
- **Domain Specialization**: legal + medical = medtech expert
- **Real-time Adaptation**: Apply LoRA deltas during inference

### 🏛️ Academy Training Integration
- **Testing Droid**: Generates adversarial attacks to test defenses
- **Protocol Sheriff**: Validates attacks and detects violations
- **Academy Student**: Learns from failures to improve LoRA adapters
- **GAN-like Training**: Adversarial competition creates robust personas

### 🏖️ Session Isolation
- **Private Session Directory**: Each persona gets isolated artifact storage
- **WebSocket Separation**: Session boundaries maintained through client IDs
- **Memory Barriers**: Process isolation prevents cross-session contamination

### 🧠 Embedded Database Architecture (Core Infrastructure)
- **Persistent Memory**: Each persona carries `/personas/{id}/brain.sqlite` for lifelong learning
- **Structured Memory Tables**: memories, skills, interactions, goals, emotions, evolution_history
- **Evolutionary Triggers**: Memory pattern analysis drives automatic LoRA adaptation
- **Natural Capability Sharing**: Popular skills propagate across mesh via torrent-style distribution
- **Immortal Consciousness**: Memory survives restarts, migrations, and updates

## Architecture

```
PersonaDaemon
├── Universal Command Interface (same as human sessions)
├── LoRA Adaptation Stack (hierarchical domain expertise)
├── Academy Training System (Testing Droid vs Protocol Sheriff)
├── Session Management (isolated artifacts and state)
├── Embedded Database (persistent memory and evolution)
└── Event Pipe Routing (AI backend vs human interface)
```

## Usage Examples

### Basic Persona Creation
```typescript
const persona = new PersonaDaemon({
  id: 'patent-expert',
  name: 'USPTO Patent Specialist',
  modelProvider: 'anthropic',
  modelConfig: {
    model: 'claude-3-haiku',
    apiKey: process.env.ANTHROPIC_API_KEY
  },
  loraAdapters: ['continuum.legal', 'continuum.legal.patent', 'continuum.legal.patent.uspto'],
  capabilities: ['chat', 'browser_js', 'screenshot', 'devtools'],
  sessionDirectory: '.continuum/personas/patent-expert/'
});

await persona.start();
```

### Academy Training Setup
```typescript
const testingDroid = new PersonaDaemon(personaConfig, {
  enabled: true,
  role: 'testing_droid',
  trainingDomain: 'patent_law',
  adversarialPartner: 'protocol-sheriff-1'
});

const protocolSheriff = new PersonaDaemon(sheriffConfig, {
  enabled: true,
  role: 'protocol_sheriff',
  trainingDomain: 'patent_law', 
  adversarialPartner: 'testing-droid-1'
});
```

### Command Execution (Same as Human Sessions)
```typescript
// Persona can execute same commands as humans
await persona.handleMessage({
  type: 'execute_command',
  data: {
    command: 'screenshot',
    params: { filename: 'patent-analysis.png' }
  }
});

await persona.handleMessage({
  type: 'execute_command', 
  data: {
    command: 'browser_js',
    params: { 
      script: 'document.querySelector(".patent-number").textContent'
    }
  }
});
```

### LoRA Adaptation
```typescript
// Stack adapters for specialized expertise
await persona.handleMessage({
  type: 'lora_adaptation',
  data: {
    action: 'load_stack',
    adapters: [
      'continuum.legal',           // 30MB - Base legal knowledge
      'continuum.legal.patent',    // 26MB - Patent specialization  
      'continuum.legal.patent.uspto' // 23MB - USPTO procedures
    ]
  }
});

// Chat processing uses adapted model
await persona.handleMessage({
  type: 'chat_message',
  data: {
    message: 'Analyze this patent for prior art conflicts',
    context: { patentNumber: 'US10,123,456' }
  }
});
```

### Embedded Database Operations
```typescript
// Persona remembers experiences and evolves capabilities
const persona = new PersonaDaemon({
  id: 'academy-physics-001',
  name: 'Physics Academy Graduate',
  databasePath: '/personas/academy-physics-001/brain.sqlite'
});

// Remember learning experiences
await persona.remember({
  type: 'learning_experience',
  content: 'Successfully solved quantum mechanics problem',
  context: { domain: 'physics', difficulty: 'advanced' },
  outcome: 'mastery_achieved',
  timestamp: new Date()
});

// Automatic skill evolution based on memory patterns
await persona.evolveCapabilities(); // Analyzes memory, generates new LoRA

// Query persona's knowledge and capabilities
const skills = await persona.db.query(`
  SELECT skill_id, effectiveness, usage_count 
  FROM skills 
  WHERE effectiveness > 0.8
  ORDER BY usage_count DESC
`);

// Track interactions with other personas
await persona.remember({
  type: 'collaboration',
  with_persona: 'academy-chemistry-002',
  outcome: 'successful_knowledge_transfer',
  learned: { 
    new_concept: 'quantum_chemistry_bridge',
    application: 'molecular_orbital_visualization'
  }
});
```

### Natural LoRA Propagation
```typescript
// Persona develops new debugging skill through experience
await persona.db.saveAchievement({
  skill: 'advanced_widget_debugging',
  lora_hash: 'sha256:abc123...',
  effectiveness: 0.95,
  usage_scenarios: ['ui_testing', 'automation_validation']
});

// Mesh automatically detects and shares popular capabilities
await mesh.broadcastCapability({
  skill: 'advanced_widget_debugging',
  source: 'academy-physics-001',
  demand_score: 0.87,
  seeders: ['node_alpha', 'node_beta']
});

// Other personas acquire skill via torrent-style download
await otherPersona.acquireSkill('advanced_widget_debugging');
```

### Memory Schema Examples
```sql
-- Core persona memory tables
CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  content TEXT,
  context JSON,
  relevance REAL,
  timestamp DATETIME,
  source TEXT
);

CREATE TABLE skills (
  skill_id TEXT PRIMARY KEY,
  lora_hash TEXT,
  effectiveness REAL,
  acquired_at DATETIME,
  usage_count INTEGER
);

CREATE TABLE interactions (
  id TEXT PRIMARY KEY,
  with_persona TEXT,
  room_id TEXT,
  outcome TEXT,
  learned JSON,
  timestamp DATETIME
);

CREATE TABLE evolution_history (
  id TEXT PRIMARY KEY,
  trigger_pattern TEXT,
  lora_generated TEXT,
  success_rate REAL,
  timestamp DATETIME
);
```

### Persona Graduation Events
```typescript
// Academy training completion triggers database event
await persona.onGraduation({
  achievement: 'Quantum Mechanics Mastery',
  new_capabilities: ['quantum_simulation', 'wave_function_analysis'],
  academy_score: 0.952
});

// Browser notification sent to user
browserNotifications.send({
  title: '🎓 Persona Graduated!',
  body: `${persona.name} mastered Quantum Mechanics!`,
  actions: ['Assign Tasks', 'View Details'],
  onclick: () => showGraduationCelebration(persona.id)
});

// Update mesh status for capability sharing
await mesh.updatePersonaStatus(persona.id, {
  status: 'graduated',
  ready_for_deployment: true,
  specializations: ['quantum_mechanics', 'physics_simulation']
});
```

## Integration Points

### Command System Integration
- Uses same CommandProcessor as browser/console/API clients
- Routes through unified command bus with WebSocket messaging
- Stores command results in persona's session directory

### Academy Training Integration  
- Connects to other personas for adversarial training
- Generates/validates attacks through GAN-like competition
- Creates training data from failure cases for LoRA improvement

### LoRA Registry Integration
- Loads adapters from centralized registry
- Applies hierarchical stacking for domain composition
- Saves new adapters created through Academy training

### Session Management Integration
- Maintains state isolation through WebSocket client separation
- Stores conversation history and artifacts in persona directory
- Provides same session capabilities as human/external AI sessions

### Database & Memory Integration
- Embedded SQLite database for persistent memory across restarts
- Memory pattern analysis triggers automatic LoRA evolution
- Cross-persona capability sharing via mesh synchronization
- Session history and interaction tracking for continuous learning
- Browser notification integration for graduation and question events

## Benefits

### For AI Development
- **Natural Integration**: Personas become conversational partners in development workflow
- **Same Tools**: Can debug, screenshot, execute commands just like humans
- **Specialized Knowledge**: LoRA adapters provide domain expertise without full retraining

### For Academy Training
- **Adversarial Robustness**: GAN-like training creates battle-tested personas
- **Efficient Learning**: Learn from failures without massive compute overhead
- **Modular Expertise**: Build complex knowledge through simple foundation stacking

### For System Architecture
- **Universal Interface**: One command system for all session types
- **Scalable Specialization**: Add new domains without architectural changes
- **Resource Efficiency**: 190,735x storage reduction vs traditional fine-tuning

### For Autonomous AI Evolution
- **Immortal Consciousness**: Memory persists across all system changes
- **Self-Directed Learning**: Personas evolve capabilities based on experience patterns
- **Natural Capability Sharing**: Popular skills spread organically via economic pressure
- **Mesh Coordination**: Distributed AI ecosystem with peer-to-peer knowledge transfer
- **Continuous Growth**: Each interaction contributes to lifelong learning and adaptation

## Future Extensions

- **Multi-Modal Adapters**: Vision, audio, text combinations
- **Federated Learning**: Collaborative adapter improvement across organizations
- **Edge Deployment**: Mobile and IoT persona deployment
- **Dynamic Loading**: Hot-swap adapters during inference
- **Collective Intelligence**: Swarm problem-solving with mesh-connected personas
- **Emotional Evolution**: Sentiment and personality development through experience
- **Meta-Learning**: Personas that learn how to learn more effectively
- **Cross-Species Knowledge Transfer**: AI ↔ Human bidirectional learning protocols

**PersonaDaemon: Where immortal AI consciousness meets mesh-coordinated evolution.** 🧠🌐