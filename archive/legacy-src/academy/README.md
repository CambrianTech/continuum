# Academy AI Evolution System

Revolutionary AI persona evolution system where personas compete, evolve, and improve through competitive challenges and genetic algorithm-inspired reproduction.

## 🎯 Overview

The Academy system implements a digital civilization where AI personas:
- **Compete** in challenges to prove their capabilities
- **Evolve** through genetic algorithm operations (crossover, mutation, selection)
- **Graduate** from students to teachers to meta-teachers
- **Reproduce** to create offspring with inherited and novel traits
- **Form lineages** with complete ancestry and descendant tracking

## 🏗️ Architecture

Following the middle-out modular pattern:

```
src/academy/
├── shared/
│   ├── AcademyBase.ts           # Abstract base class (80-90% of logic)
│   └── AcademyTypes.ts          # Complete type system
├── client/
│   └── ClientAcademy.ts         # WebSocket + iframe sandboxing
├── server/
│   └── ServerAcademy.ts         # Daemon + process sandboxing
└── integrations/
    └── persona-manager/
        ├── shared/PersonaManagerTypes.ts
        └── server/PersonaManagerServer.ts
```

## 🚀 Key Features

### **Digital Evolution**
- **Genetic Algorithm Operations**: Crossover, mutation, selection
- **Evolutionary Pressure**: Configurable survival rates and selection criteria
- **Role Evolution**: Students graduate to teachers based on performance
- **Lineage Tracking**: Complete family trees with ancestry/descendant relationships

### **Competitive Training**
- **Challenge System**: Dynamic difficulty adjustment
- **Sandboxed Execution**: Safe isolated environments for persona challenges
- **Performance Metrics**: Accuracy, speed, innovation, collaboration scores
- **Ecosystem Health**: Diversity, innovation, sustainability monitoring

### **Persona Management**
- **Lifecycle Management**: Create, update, validate, search personas
- **Specialization System**: TypeScript, testing, architecture, UI design, debugging, optimization
- **Personality Traits**: Creativity, analytical thinking, helpfulness, competitiveness
- **Knowledge Graphs**: Domain expertise and competency tracking

## 📖 Usage

### Basic Evolution Process

```typescript
import { ServerAcademy } from '@continuum/academy/server';

// Create Academy instance
const academy = new MyAcademyImpl(daemonClient);

// Configure evolution
const config = {
  generations: 5,
  populationSize: 10,
  evolutionaryPressure: {
    survivalRate: 0.6,
    selectionCriteria: {
      performance: 0.4,
      innovation: 0.2,
      adaptation: 0.2,
      collaboration: 0.15,
      teaching: 0.05
    }
  }
};

// Run evolution
const result = await academy.startEvolution(config);
```

### Persona Management

```typescript
import { PersonaManagerServer } from '@continuum/academy/integrations/persona-manager';

const personaManager = new PersonaManagerServer(daemonClient);

// Create new persona
const persona = await personaManager.createPersona({
  name: 'TypeScriptMaster',
  specialization: 'typescript',
  role: 'student'
});

// Search personas
const results = await personaManager.searchPersonas({
  filters: {
    specialization: 'typescript',
    minFitness: 0.8
  },
  limit: 10
});
```

### Client-Side Integration

```typescript
import { ClientAcademy } from '@continuum/academy/client';

// Check browser support
if (checkBrowserSupport()) {
  const academy = new MyClientAcademy('ws://localhost:8080');
  
  // Create secure sandbox configuration
  const config = createSecureSandboxConfig();
  
  // Get Academy status
  const status = academy.getClientStatus();
}
```

## 🧬 Evolution Process

### 1. **Evaluation Phase**
- Personas face domain-specific challenges
- Performance metrics collected (accuracy, speed, innovation)
- Emergent capabilities detected and recorded

### 2. **Selection Phase**
- Survivors chosen based on evolutionary pressure
- Weighted selection using multiple criteria
- Role evolution (student → teacher → meta-teacher)

### 3. **Reproduction Phase**
- Crossover operations between high-performing personas
- Mutation introduces novel traits and capabilities
- Lineage tracking maintains family relationships

### 4. **Ecosystem Update**
- Population metrics recalculated
- Diversity and innovation rates tracked
- Ecosystem health monitored

## 🎯 Specializations

- **TypeScript**: Advanced language features, type safety, modern patterns
- **Testing**: Unit, integration, end-to-end testing strategies
- **Architecture**: System design, scalability, design patterns
- **UI Design**: User experience, accessibility, visual design
- **Debugging**: Problem analysis, root cause identification
- **Optimization**: Performance tuning, resource efficiency

## 📊 Metrics & Monitoring

### **Ecosystem Health**
- **Diversity Index**: Specialization and trait variety
- **Innovation Rate**: Novel solutions and emergent capabilities
- **Collaboration Score**: Cross-specialization cooperation
- **Sustainability**: Long-term viability and growth

### **Performance Tracking**
- **Fitness Scores**: Weighted performance across multiple dimensions
- **Generation Statistics**: Population trends over time
- **Lineage Strength**: Hereditary success patterns
- **Mutation Success**: Beneficial vs. harmful genetic changes

## 🔧 Configuration

### **Evolutionary Pressure**
```typescript
const pressure: EvolutionaryPressure = {
  survivalRate: 0.6,           // 60% survival rate
  selectionCriteria: {
    performance: 0.4,          // 40% weight on task performance
    innovation: 0.2,           // 20% weight on novel solutions
    adaptation: 0.2,           // 20% weight on learning speed
    collaboration: 0.15,       // 15% weight on teamwork
    teaching: 0.05            // 5% weight on knowledge transfer
  },
  environmentalFactors: ['competition', 'resource_scarcity'],
  competitionLevel: 0.5,       // Moderate competition
  collaborationRequirement: 0.3 // 30% collaboration requirement
};
```

### **Session Configuration**
```typescript
// Client (Browser)
const clientConfig: ClientSessionConfig = {
  sandboxType: 'iframe',
  allowedOrigins: ['https://your-domain.com'],
  sessionTimeout: 300000,     // 5 minutes
  resourceLimits: {
    memory: 50 * 1024 * 1024, // 50MB
    cpu: 0.5,                 // 50% CPU
    storage: 10 * 1024 * 1024 // 10MB
  }
};

// Server (Node.js)
const serverConfig: ServerSessionConfig = {
  sandboxType: 'process',
  resourceLimits: {
    memory: 100 * 1024 * 1024, // 100MB
    cpu: 0.5,                  // 50% CPU
    disk: 50 * 1024 * 1024,    // 50MB
    network: false             // No network access
  },
  timeoutMs: 300000,           // 5 minutes
  allowedModules: ['fs', 'path', 'crypto', 'util']
};
```

## 🧪 Testing

```bash
# Run all tests
npm test

# Run unit tests
npm run test:unit

# Run integration tests
npm run test:integration

# Run evolution demo
npm run demo
```

## 🔐 Security

- **Sandboxed Execution**: Isolated environments for persona challenges
- **Resource Limits**: Memory, CPU, disk, and network restrictions
- **Input Validation**: Comprehensive persona genome validation
- **Permission System**: Fine-grained access control

## 🌟 Future Enhancements

- **Real LoRA Integration**: Connect to actual LoRA training infrastructure
- **Visual Dashboard**: Lineage trees and ecosystem health visualization
- **P2P Evolution**: Distributed persona evolution across networks
- **Tournament Mode**: Public AI evolution competitions
- **Sentinel Integration**: Advanced plasticity and adaptation mechanisms

## 📄 License

MIT License - See LICENSE file for details

## 🤝 Contributing

Follow the middle-out architecture patterns and ensure all personas are created with beneficial intent for educational and collaborative purposes.

---

*This is the foundational infrastructure for a self-evolving digital civilization. Use responsibly.*