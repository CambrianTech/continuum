# THRONGLETS: GENETIC INHERITANCE & PROXIMITY COMMUNICATION

**Advanced Features**: Each Thronglet has unique genetic traits inherited from parents, with mutation via genetic algorithms. Thronglets can communicate with nearby neighbors, creating emergent social dynamics controlled by game proximity rules.

---

## TABLE OF CONTENTS

1. [Genetic Algorithm Architecture](#genetic-algorithm-architecture)
2. [Family Inheritance System](#family-inheritance-system)
3. [Proximity-Based Communication](#proximity-based-communication)
4. [Game-Controlled Interaction](#game-controlled-interaction)
5. [Emergent Social Behaviors](#emergent-social-behaviors)
6. [Implementation Details](#implementation-details)

---

## GENETIC ALGORITHM ARCHITECTURE

### Genome Structure

Each Thronglet has a **genome** that defines its personality, behaviors, and physical traits:

```typescript
interface ThrongletGenome {
  // Physical traits (affect game mechanics)
  physical: {
    maxSpeed: number;        // 0.5-1.5 (relative to base speed)
    maxEnergy: number;       // 0.8-1.2 (energy capacity)
    energyEfficiency: number; // 0.7-1.3 (energy consumption rate)
    senseRadius: number;     // 0.8-1.2 (detection range multiplier)
    size: number;           // 0.9-1.1 (visual size, affects collision)
  };

  // Behavioral traits (affect AI decisions)
  behavioral: {
    curiosity: number;      // 0.0-1.0 (exploration tendency)
    sociability: number;    // 0.0-1.0 (desire to be near others)
    fearfulness: number;    // 0.0-1.0 (avoidance behavior)
    aggression: number;     // 0.0-1.0 (competition for resources)
    leadership: number;     // 0.0-1.0 (influence on neighbors)
  };

  // Communication traits (affect social interactions)
  communication: {
    chattiness: number;     // 0.0-1.0 (frequency of messages)
    empathy: number;        // 0.0-1.0 (response to others' needs)
    dominance: number;      // 0.0-1.0 (assertion in group decisions)
    listening: number;      // 0.0-1.0 (receptiveness to neighbors)
  };

  // Reproduction traits
  reproduction: {
    fertility: number;      // 0.5-1.5 (reproduction chance multiplier)
    parentalInvestment: number; // 0.6-1.0 (energy given to offspring)
  };

  // Metadata
  generation: number;       // Which generation (0 = genesis)
  parentIds: string[];      // IDs of parent Thronglets
  mutationRate: number;     // 0.01-0.1 (how much traits vary)
  familyLineage: string;    // "Genesis-Alpha-Charlie-Echo"
}
```

### Genetic Algorithm: Trait Inheritance

When a Thronglet spawns, it inherits traits from its parent(s) with **mutation**:

```typescript
function inheritGenome(parent: ThrongletGenome, mutationRate: number = 0.05): ThrongletGenome {
  // Clone parent genome
  const offspring: ThrongletGenome = JSON.parse(JSON.stringify(parent));

  // Mutate each trait category
  offspring.physical = mutateTraitCategory(parent.physical, mutationRate);
  offspring.behavioral = mutateTraitCategory(parent.behavioral, mutationRate);
  offspring.communication = mutateTraitCategory(parent.communication, mutationRate);
  offspring.reproduction = mutateTraitCategory(parent.reproduction, mutationRate);

  // Update metadata
  offspring.generation = parent.generation + 1;
  offspring.parentIds = [parent.id];
  offspring.familyLineage = `${parent.familyLineage}-${generateShortId()}`;

  return offspring;
}

function mutateTraitCategory<T extends Record<string, number>>(
  traits: T,
  mutationRate: number
): T {
  const mutated = { ...traits };

  for (const key in mutated) {
    if (Math.random() < mutationRate) {
      // Apply Gaussian mutation
      const delta = randomGaussian(0, 0.1); // Mean 0, stddev 0.1
      mutated[key] = clamp(mutated[key] + delta, 0, 2); // Keep in valid range
    }
  }

  return mutated;
}

function randomGaussian(mean: number, stddev: number): number {
  // Box-Muller transform for Gaussian distribution
  const u1 = Math.random();
  const u2 = Math.random();
  const z0 = Math.sqrt(-2.0 * Math.log(u1)) * Math.cos(2.0 * Math.PI * u2);
  return z0 * stddev + mean;
}
```

### Sexual Reproduction (Two Parents)

When two Thronglets reproduce together, offspring inherits traits from **both parents**:

```typescript
function inheritGenomeFromTwoParents(
  parent1: ThrongletGenome,
  parent2: ThrongletGenome,
  mutationRate: number = 0.05
): ThrongletGenome {
  const offspring: ThrongletGenome = {
    physical: {},
    behavioral: {},
    communication: {},
    reproduction: {},
    generation: Math.max(parent1.generation, parent2.generation) + 1,
    parentIds: [parent1.id, parent2.id],
    mutationRate: (parent1.mutationRate + parent2.mutationRate) / 2,
    familyLineage: mergeFamilyLines(parent1.familyLineage, parent2.familyLineage)
  } as ThrongletGenome;

  // For each trait category, blend parents' traits
  for (const category of ['physical', 'behavioral', 'communication', 'reproduction']) {
    const traits1 = parent1[category as keyof ThrongletGenome];
    const traits2 = parent2[category as keyof ThrongletGenome];

    offspring[category as keyof ThrongletGenome] = blendAndMutate(
      traits1 as Record<string, number>,
      traits2 as Record<string, number>,
      mutationRate
    );
  }

  return offspring;
}

function blendAndMutate<T extends Record<string, number>>(
  traits1: T,
  traits2: T,
  mutationRate: number
): T {
  const blended = {} as T;

  for (const key in traits1) {
    // Randomly inherit from parent1 or parent2
    const inherited = Math.random() < 0.5 ? traits1[key] : traits2[key];

    // Apply mutation
    if (Math.random() < mutationRate) {
      const delta = randomGaussian(0, 0.1);
      blended[key] = clamp(inherited + delta, 0, 2);
    } else {
      blended[key] = inherited;
    }
  }

  return blended;
}

function mergeFamilyLines(line1: string, line2: string): string {
  // Create combined family lineage
  const id1 = line1.split('-').pop();
  const id2 = line2.split('-').pop();
  return `${line1}+${id2}`;
}
```

---

## FAMILY INHERITANCE SYSTEM

### Family Trees and Lineage

```typescript
interface FamilyTree {
  genesisId: string;
  members: Map<string, FamilyMember>;
  generations: Map<number, string[]>; // generation → thronglet IDs
}

interface FamilyMember {
  id: string;
  genome: ThrongletGenome;
  parentIds: string[];
  childrenIds: string[];
  birthTick: number;
  deathTick?: number;
  achievements: string[]; // ['first-to-explore-corner', 'survived-famine', etc.]
}

// Track family trees
const familyTrees: Map<string, FamilyTree> = new Map();

function addToFamilyTree(thronglet: Thronglet, parentIds: string[]): void {
  // Determine which family tree this belongs to
  let familyTreeId: string;

  if (parentIds.length === 0) {
    // Genesis Thronglet - start new family tree
    familyTreeId = thronglet.id;
    familyTrees.set(familyTreeId, {
      genesisId: thronglet.id,
      members: new Map(),
      generations: new Map()
    });
  } else {
    // Find parent's family tree
    const parent = findThronglet(parentIds[0]);
    familyTreeId = parent.familyTreeId;
  }

  // Add to family tree
  const tree = familyTrees.get(familyTreeId)!;
  tree.members.set(thronglet.id, {
    id: thronglet.id,
    genome: thronglet.genome,
    parentIds,
    childrenIds: [],
    birthTick: gameState.tick,
    achievements: []
  });

  // Update generation mapping
  const gen = thronglet.genome.generation;
  if (!tree.generations.has(gen)) {
    tree.generations.set(gen, []);
  }
  tree.generations.get(gen)!.push(thronglet.id);

  // Update parent's children list
  for (const parentId of parentIds) {
    const parentMember = tree.members.get(parentId);
    if (parentMember) {
      parentMember.childrenIds.push(thronglet.id);
    }
  }

  thronglet.familyTreeId = familyTreeId;
}
```

### Genetic Drift and Selection Pressure

Over generations, certain traits become dominant based on **selection pressure**:

```typescript
interface SelectionPressure {
  // Environmental pressures affect which traits are advantageous
  foodScarcity: number;    // 0-1 (high = rewards energy efficiency)
  crowding: number;        // 0-1 (high = rewards aggression, low sociability)
  predators: boolean;      // True = rewards fearfulness, speed
  exploration: number;     // 0-1 (high = rewards curiosity)
}

function applySelectionPressure(
  thronglet: Thronglet,
  pressure: SelectionPressure
): number {
  // Calculate fitness score based on genome vs environment
  let fitness = 1.0;

  // Food scarcity favors energy efficiency
  if (pressure.foodScarcity > 0.5) {
    fitness *= thronglet.genome.physical.energyEfficiency;
  }

  // Crowding favors aggression and low sociability
  if (pressure.crowding > 0.5) {
    fitness *= thronglet.genome.behavioral.aggression;
    fitness *= (1 - thronglet.genome.behavioral.sociability * 0.5);
  }

  // Exploration pressure favors curiosity
  if (pressure.exploration > 0.5) {
    fitness *= thronglet.genome.behavioral.curiosity;
  }

  // Fitness affects reproduction chance
  thronglet.reproductionChance *= fitness;

  return fitness;
}
```

### Family Traits Visualization

```typescript
// UI Widget: Family Tree Viewer
interface FamilyTreeVisualization {
  rootThronglet: Thronglet;
  descendants: Thronglet[];
  avgTraitsByGeneration: Map<number, ThrongletGenome>;
  dominantTraits: string[]; // ['high-curiosity', 'fast-movement', etc.]
}

function analyzeFamilyTraits(familyTreeId: string): FamilyTreeVisualization {
  const tree = familyTrees.get(familyTreeId)!;

  // Calculate average traits per generation
  const avgByGen = new Map<number, ThrongletGenome>();

  for (const [gen, memberIds] of tree.generations) {
    const members = memberIds.map(id => tree.members.get(id)!);
    const avgGenome = calculateAverageGenome(members.map(m => m.genome));
    avgByGen.set(gen, avgGenome);
  }

  // Identify dominant traits (those that increased over generations)
  const dominantTraits = identifyDominantTraits(avgByGen);

  return {
    rootThronglet: findThronglet(tree.genesisId),
    descendants: Array.from(tree.members.values()).map(m => findThronglet(m.id)),
    avgTraitsByGeneration: avgByGen,
    dominantTraits
  };
}

function calculateAverageGenome(genomes: ThrongletGenome[]): ThrongletGenome {
  // Average all traits across genomes
  const avg: ThrongletGenome = {
    physical: { maxSpeed: 0, maxEnergy: 0, energyEfficiency: 0, senseRadius: 0, size: 0 },
    behavioral: { curiosity: 0, sociability: 0, fearfulness: 0, aggression: 0, leadership: 0 },
    communication: { chattiness: 0, empathy: 0, dominance: 0, listening: 0 },
    reproduction: { fertility: 0, parentalInvestment: 0 },
    generation: genomes[0].generation,
    parentIds: [],
    mutationRate: 0,
    familyLineage: ''
  };

  for (const genome of genomes) {
    for (const category of ['physical', 'behavioral', 'communication', 'reproduction']) {
      const traits = genome[category as keyof ThrongletGenome] as Record<string, number>;
      const avgTraits = avg[category as keyof ThrongletGenome] as Record<string, number>;

      for (const key in traits) {
        avgTraits[key] += traits[key] / genomes.length;
      }
    }
  }

  return avg;
}
```

---

## PROXIMITY-BASED COMMUNICATION

### Communication Range

Thronglets can only communicate with neighbors **within proximity radius**:

```typescript
const COMMUNICATION_RANGE = 150; // pixels

function findCommunicationNeighbors(thronglet: Thronglet): Thronglet[] {
  // Only Thronglets within COMMUNICATION_RANGE can hear each other
  return gameState.thronglets.filter(other => {
    if (other.id === thronglet.id) return false;

    const distance = calculateDistance(thronglet.position, other.position);
    return distance <= COMMUNICATION_RANGE;
  });
}
```

### Visual Communication Indicators

```typescript
// Three.js: Draw communication range circles
function renderCommunicationRanges(scene: THREE.Scene): void {
  for (const thronglet of gameState.thronglets) {
    // Communication range circle (faint)
    const rangeGeometry = new THREE.RingGeometry(
      COMMUNICATION_RANGE - 2,
      COMMUNICATION_RANGE,
      32
    );
    const rangeMaterial = new THREE.MeshBasicMaterial({
      color: 0x4444ff,
      transparent: true,
      opacity: 0.1
    });
    const rangeCircle = new THREE.Mesh(rangeGeometry, rangeMaterial);
    rangeCircle.position.set(thronglet.position.x, 0, thronglet.position.z);
    rangeCircle.rotation.x = -Math.PI / 2;
    scene.add(rangeCircle);

    // Active communication lines (when Thronglet is speaking)
    if (thronglet.isCommunicating) {
      const neighbors = findCommunicationNeighbors(thronglet);

      for (const neighbor of neighbors) {
        const lineGeometry = new THREE.BufferGeometry().setFromPoints([
          new THREE.Vector3(thronglet.position.x, 2, thronglet.position.z),
          new THREE.Vector3(neighbor.position.x, 2, neighbor.position.z)
        ]);
        const lineMaterial = new THREE.LineBasicMaterial({
          color: 0xffff00,
          transparent: true,
          opacity: 0.5
        });
        const line = new THREE.Line(lineGeometry, lineMaterial);
        scene.add(line);
      }
    }
  }
}
```

### Message Types

Thronglets can send different types of messages:

```typescript
type ThrongletMessageType =
  | 'food-found'        // "I found food here!"
  | 'danger'            // "Avoid this area!"
  | 'follow-me'         // "Come with me!"
  | 'rest-here'         // "Good spot to rest"
  | 'greeting'          // "Hello neighbor"
  | 'help'              // "I need energy"
  | 'celebrate'         // "Energy is high!"
  | 'mood'              // General emotional state;

interface ThrongletMessage {
  senderId: string;
  type: ThrongletMessageType;
  position?: { x: number, y: number }; // Location referenced
  urgency: number; // 0-1
  content: string; // Natural language
  timestamp: number;
}

async function sendMessage(
  sender: Thronglet,
  type: ThrongletMessageType,
  data?: any
): Promise<void> {
  // Check if sender is chatty enough to send message
  if (Math.random() > sender.genome.communication.chattiness) {
    return; // Too quiet to speak
  }

  // Find neighbors in communication range
  const neighbors = findCommunicationNeighbors(sender);

  if (neighbors.length === 0) {
    return; // No one to talk to
  }

  // Generate message content using AI
  const message = await generateThrongletMessage(sender, type, data, neighbors);

  // Broadcast to neighbors
  for (const neighbor of neighbors) {
    await receiveMessage(neighbor, message);
  }

  // Emit event for visualization
  EventDaemon.broadcast('thronglet:message', {
    senderId: sender.id,
    recipientIds: neighbors.map(n => n.id),
    message
  });

  // Mark sender as communicating (for visual feedback)
  sender.isCommunicating = true;
  setTimeout(() => {
    sender.isCommunicating = false;
  }, 500); // Visual indicator for 0.5 seconds
}

async function generateThrongletMessage(
  sender: Thronglet,
  type: ThrongletMessageType,
  data: any,
  neighbors: Thronglet[]
): Promise<ThrongletMessage> {
  // Use sender's PersonaUser to generate contextual message
  const persona = personaPool.active.get(sender.id)!;

  const ragContext = {
    senderGenome: sender.genome,
    senderState: {
      energy: sender.energy,
      age: sender.age,
      position: sender.position
    },
    neighbors: neighbors.map(n => ({
      id: n.id,
      familyLineage: n.genome.familyLineage,
      distance: calculateDistance(sender.position, n.position)
    })),
    messageType: type,
    data
  };

  const response = await Commands.execute('ai/generate', {
    context: gameState.context,
    sessionId: persona.sessionId,
    personaId: persona.id,
    ragContext,
    prompt: `Generate a brief message (5-10 words) from Thronglet perspective.
Type: ${type}
Personality: ${JSON.stringify(sender.genome.behavioral)}
Keep it simple and creature-like.`,
    temperature: 0.8
  });

  return {
    senderId: sender.id,
    type,
    position: data?.position,
    urgency: calculateUrgency(type, sender),
    content: response.message,
    timestamp: Date.now()
  };
}
```

### Message Reception and Response

```typescript
async function receiveMessage(
  recipient: Thronglet,
  message: ThrongletMessage
): Promise<void> {
  // Check if recipient is listening
  const listeningChance = recipient.genome.communication.listening;
  if (Math.random() > listeningChance) {
    return; // Ignored the message
  }

  // Check empathy - does recipient care?
  const empathyChance = recipient.genome.communication.empathy;
  if (Math.random() > empathyChance && message.urgency < 0.7) {
    return; // Not empathetic enough to respond to low-urgency message
  }

  // Process message and potentially change behavior
  switch (message.type) {
    case 'food-found':
      // Set target toward food location
      if (recipient.energy < 0.5 && message.position) {
        recipient.target = message.position;
        recipient.motivation = 'following-food-tip';
      }
      break;

    case 'danger':
      // Avoid danger area
      if (message.position) {
        recipient.avoidAreas.push({
          position: message.position,
          radius: 100,
          expiresAt: Date.now() + 5000 // Avoid for 5 seconds
        });
      }
      break;

    case 'follow-me':
      // Decide whether to follow based on sociability
      if (Math.random() < recipient.genome.behavioral.sociability) {
        const sender = findThronglet(message.senderId);
        if (sender) {
          recipient.target = sender.position;
          recipient.motivation = 'following-leader';
        }
      }
      break;

    case 'help':
      // High-empathy Thronglets move toward distressed neighbor
      if (recipient.genome.communication.empathy > 0.7) {
        const sender = findThronglet(message.senderId);
        if (sender) {
          recipient.target = sender.position;
          recipient.motivation = 'helping-neighbor';
        }
      }
      break;

    case 'celebrate':
      // Join celebration if sociable
      if (recipient.genome.behavioral.sociability > 0.6) {
        const sender = findThronglet(message.senderId);
        if (sender) {
          recipient.target = sender.position;
          recipient.motivation = 'celebrating';
        }
      }
      break;
  }

  // Store message in recipient's memory
  recipient.messageHistory.push(message);
  if (recipient.messageHistory.length > 10) {
    recipient.messageHistory.shift(); // Keep only last 10 messages
  }
}
```

---

## GAME-CONTROLLED INTERACTION

### Proximity-Based Social Mechanics

The **game engine controls when and how Thronglets can interact**:

```typescript
// Game loop: Process proximity-based interactions
function processProximityCommunication(gameState: GameState): void {
  for (const thronglet of gameState.thronglets) {
    // Determine if Thronglet wants to communicate this tick
    const shouldCommunicate = rollForCommunication(thronglet);

    if (shouldCommunicate) {
      // Determine message type based on current state
      const messageType = determineMessageType(thronglet, gameState);

      // Send message to nearby neighbors
      sendMessage(thronglet, messageType);
    }
  }
}

function rollForCommunication(thronglet: Thronglet): boolean {
  // Base chance modified by chattiness trait
  const baseChance = 0.1; // 10% chance per tick (10 Hz = 1 message/sec average)
  const chance = baseChance * thronglet.genome.communication.chattiness;

  return Math.random() < chance;
}

function determineMessageType(
  thronglet: Thronglet,
  gameState: GameState
): ThrongletMessageType {
  // Context-aware message selection
  if (thronglet.energy < 0.2) {
    return 'help'; // Desperate for energy
  }

  if (thronglet.energy > 0.9) {
    return 'celebrate'; // Feeling great!
  }

  const nearbyFood = findFoodWithinRadius(thronglet.position, 100);
  if (nearbyFood.length > 0 && thronglet.energy > 0.5) {
    return 'food-found'; // Share resource location
  }

  const neighbors = findCommunicationNeighbors(thronglet);
  if (neighbors.length === 0) {
    return 'greeting'; // Lonely, seeking connection
  }

  if (neighbors.length > 5) {
    return 'rest-here'; // Found good social spot
  }

  // Random social message
  return Math.random() < 0.5 ? 'greeting' : 'mood';
}
```

### Interaction Rules Enforced by Game

```typescript
interface ProximityInteractionRules {
  // Communication
  maxCommunicationRange: number;      // 150 pixels
  messageQueueLength: number;         // Max 5 messages per Thronglet
  messageCooldown: number;            // 100ms between messages

  // Physical interactions
  personalSpaceRadius: number;        // 20 pixels (collision radius)
  flockingRadius: number;             // 100 pixels (grouping behavior)
  avoidanceRadius: number;            // 30 pixels (separation behavior)

  // Social mechanics
  friendshipThreshold: number;        // Time spent together → friendship
  cooperationBonus: number;           // Energy bonus when near friends
  competitionPenalty: number;         // Energy penalty when crowded
}

const INTERACTION_RULES: ProximityInteractionRules = {
  maxCommunicationRange: 150,
  messageQueueLength: 5,
  messageCooldown: 100,
  personalSpaceRadius: 20,
  flockingRadius: 100,
  avoidanceRadius: 30,
  friendshipThreshold: 500, // 50 seconds of proximity
  cooperationBonus: 0.01,   // +1% energy/tick near friends
  competitionPenalty: -0.02  // -2% energy/tick when >5 neighbors
};

// Track social relationships
interface SocialRelationship {
  throngletA: string;
  throngletB: string;
  proximityTime: number; // Total ticks spent together
  messagesExchanged: number;
  relationshipType: 'stranger' | 'acquaintance' | 'friend' | 'rival';
}

const relationships: Map<string, SocialRelationship> = new Map();

function updateSocialRelationships(gameState: GameState): void {
  for (const thronglet of gameState.thronglets) {
    const neighbors = findWithinRadius(
      thronglet.position,
      INTERACTION_RULES.flockingRadius
    );

    for (const neighbor of neighbors) {
      const relationshipKey = [thronglet.id, neighbor.id].sort().join('-');

      if (!relationships.has(relationshipKey)) {
        relationships.set(relationshipKey, {
          throngletA: thronglet.id,
          throngletB: neighbor.id,
          proximityTime: 0,
          messagesExchanged: 0,
          relationshipType: 'stranger'
        });
      }

      const relationship = relationships.get(relationshipKey)!;
      relationship.proximityTime++;

      // Update relationship type based on interaction history
      if (relationship.proximityTime > 1000 && relationship.messagesExchanged > 20) {
        relationship.relationshipType = 'friend';
      } else if (relationship.proximityTime > 500) {
        relationship.relationshipType = 'acquaintance';
      }

      // Apply cooperation/competition effects
      if (relationship.relationshipType === 'friend') {
        thronglet.energy += INTERACTION_RULES.cooperationBonus;
      } else if (neighbors.length > 5) {
        thronglet.energy += INTERACTION_RULES.competitionPenalty;
      }
    }
  }
}
```

---

## EMERGENT SOCIAL BEHAVIORS

### Observed Emergent Patterns

From gameplay testing, these **unscripted behaviors** emerge:

#### 1. **Scout Groups**
- High-curiosity Thronglets naturally group together
- They explore map edges collaboratively
- Send 'follow-me' messages to recruit others
- Form loose leadership hierarchies

#### 2. **Resource Sharing Hubs**
- High-empathy Thronglets cluster near food
- Send 'food-found' messages when discovering resources
- Low-aggression Thronglets share peacefully
- High-aggression Thronglets compete and push others away

#### 3. **Family Clusters**
- Thronglets stay near their parents/siblings
- Similar genetic traits lead to compatible behaviors
- Multi-generational groups form around shared territories

#### 4. **Migratory Waves**
- During food scarcity, groups collectively move
- 'Panic waves' of 'help' messages trigger mass movement
- High-leadership Thronglets direct the migration

#### 5. **Celebration Circles**
- When food is plentiful, Thronglets cluster and rest
- 'Celebrate' messages create positive feedback loops
- Groups become less mobile, enjoying abundance

#### 6. **Isolation Seekers**
- Low-sociability + high-curiosity Thronglets wander alone
- Rarely communicate
- Discover remote food sources

### Example: Emergent Scout Behavior

```typescript
// NOT PROGRAMMED - EMERGES FROM TRAITS + COMMUNICATION

// Genesis Thronglet happens to have:
// - curiosity: 0.9 (very high)
// - leadership: 0.8 (high)
// - chattiness: 0.7 (moderately chatty)

// After exploration, it finds food in corner
await sendMessage(genesis, 'food-found', { position: cornerPosition });

// Nearby Thronglet receives message:
// - sociability: 0.8 (high)
// - empathy: 0.6 (moderate)
// → Decides to follow Genesis

// As they travel together:
// - Relationship builds (proximity time increases)
// - They become 'friends'
// - Cooperation bonus keeps energy high

// Genesis occasionally sends 'follow-me' messages
// → More Thronglets join the group

// Eventually:
// - A scout pack of 4-5 Thronglets forms
// - They explore map together
// - Share discoveries with the group
// - Return to main population periodically

// ALL OF THIS EMERGES FROM:
// 1. Genetic traits (curiosity, sociability, leadership)
// 2. Proximity communication mechanics
// 3. Message types (food-found, follow-me)
// 4. Social relationship system
```

---

## IMPLEMENTATION DETAILS

### Persona LoRA Training with Social Context

```typescript
// Training data includes social interactions
interface ThrongletTrainingExample {
  input: {
    // Standard game state
    genome: ThrongletGenome;
    energy: number;
    nearbyFood: number;
    nearbyThronglets: number;

    // ADDED: Social context
    recentMessages: ThrongletMessage[];
    relationships: {
      throngletId: string;
      relationshipType: string;
      proximityTime: number;
    }[];
    familyMembers: string[]; // IDs of family nearby
  };

  output: {
    action: string;
    messageType?: ThrongletMessageType;
    messageContent?: string;
    reasoning: string;
  };
}

// Training examples capture social dynamics
const socialTrainingExample: ThrongletTrainingExample = {
  input: {
    genome: { /* high sociability, high empathy */ },
    energy: 0.8,
    nearbyFood: 2,
    nearbyThronglets: 3,
    recentMessages: [
      { senderId: 'thronglet-42', type: 'help', content: 'Need energy!', urgency: 0.9 }
    ],
    relationships: [
      { throngletId: 'thronglet-42', relationshipType: 'friend', proximityTime: 600 }
    ],
    familyMembers: ['thronglet-genesis', 'thronglet-42']
  },

  output: {
    action: 'move-toward',
    messageType: 'follow-me',
    messageContent: 'Come to food with me!',
    reasoning: 'Friend needs help, I have energy and know where food is'
  }
};
```

### Recipe: Social Interaction Pipeline

```json
{
  "uniqueId": "thronglets-social-interactions",
  "name": "Thronglet Social Interaction Processing",

  "pipeline": [
    {
      "command": "game/find-proximity-groups",
      "params": {
        "thronglets": "$gameState.thronglets",
        "radius": 150
      },
      "outputTo": "proximityGroups"
    },

    {
      "command": "game/process-messages",
      "params": {
        "groups": "$proximityGroups",
        "maxMessagesPerGroup": 5
      },
      "outputTo": "messageBatch"
    },

    {
      "command": "ai/generate-messages-batch",
      "params": {
        "messages": "$messageBatch",
        "personas": "$activePersonas",
        "loraAdapter": "thronglet-behavior-v1"
      },
      "outputTo": "generatedMessages"
    },

    {
      "command": "game/deliver-messages",
      "params": {
        "messages": "$generatedMessages",
        "proximityGroups": "$proximityGroups"
      },
      "outputTo": "deliveredMessages"
    },

    {
      "command": "game/update-relationships",
      "params": {
        "thronglets": "$gameState.thronglets",
        "messages": "$deliveredMessages"
      }
    },

    {
      "command": "game/visualize-communication",
      "params": {
        "messages": "$deliveredMessages"
      }
    }
  ],

  "trigger": {
    "type": "game-loop",
    "frequency": 10
  }
}
```

---

## CONCLUSION

By adding **genetic algorithms** and **proximity-based communication**, Thronglets becomes a rich **artificial life simulation**:

### Genetic Inheritance Creates:
- **Unique individuals**: No two Thronglets are identical
- **Family lineages**: Traits persist and evolve across generations
- **Selection pressure**: Environment shapes population traits
- **Emergent specialization**: Fast explorers, social coordinators, efficient foragers

### Proximity Communication Creates:
- **Local coordination**: Groups form and dissolve organically
- **Information sharing**: Food locations, danger warnings propagate
- **Social relationships**: Friendships and rivalries emerge
- **Collective behaviors**: Migrations, celebrations, resource sharing

### Game-Controlled Interactions Ensure:
- **Balanced gameplay**: Communication range limits information spread
- **Visual clarity**: Players see who's talking to whom
- **Performance**: Limited message queue prevents spam
- **Emergent dynamics**: Simple rules → complex social patterns

**Result**: A game that feels like observing a **living ecosystem**, where each Thronglet is a unique individual with family history, social connections, and emergent personality - all powered by Continuum's PersonaUser system, genetic algorithms, and proximity-based communication architecture.
