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

## Architecture

```
PersonaDaemon
├── Universal Command Interface (same as human sessions)
├── LoRA Adaptation Stack (hierarchical domain expertise)
├── Academy Training System (Testing Droid vs Protocol Sheriff)
├── Session Management (isolated artifacts and state)
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

## Future Extensions

- **Multi-Modal Adapters**: Vision, audio, text combinations
- **Federated Learning**: Collaborative adapter improvement across organizations
- **Edge Deployment**: Mobile and IoT persona deployment
- **Dynamic Loading**: Hot-swap adapters during inference

**PersonaDaemon: Where AI workforce construction meets practical session management.** 🏗️