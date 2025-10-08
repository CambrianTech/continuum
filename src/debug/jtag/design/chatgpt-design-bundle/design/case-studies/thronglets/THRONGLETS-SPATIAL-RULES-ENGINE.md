# THRONGLETS: SPATIAL RULES ENGINE

**Core Design**: Like Warcraft and RTS games, Thronglets use **spatial state** to dictate all interactions. The environment is dynamic, with changing terrain, resources, hazards, and conditions that Thronglets must respond to. The game engine enforces rules based on **proximity, line-of-sight, terrain type, and environmental state**.

---

## TABLE OF CONTENTS

1. [Spatial State System](#spatial-state-system)
2. [Dynamic Environment](#dynamic-environment)
3. [RTS-Style Interaction Rules](#rts-style-interaction-rules)
4. [Environmental Response Behaviors](#environmental-response-behaviors)
5. [Visual Feedback System](#visual-feedback-system)
6. [Implementation](#implementation)

---

## SPATIAL STATE SYSTEM

### Grid-Based World Representation

```typescript
interface WorldGrid {
  width: number;
  height: number;
  cellSize: number; // Each cell = 50x50 pixels
  cells: Map<string, GridCell>; // "x,y" → GridCell
}

interface GridCell {
  position: { x: number; y: number };
  terrain: TerrainType;
  resources: Resource[];
  hazards: Hazard[];
  occupants: string[]; // Thronglet IDs in this cell
  passable: boolean;
  visibility: number; // 0-1 (fog of war)
  environmentalEffects: EnvironmentalEffect[];
}

type TerrainType =
  | 'grass'        // Normal movement
  | 'forest'       // Slows movement, provides cover
  | 'water'        // Impassable (for now)
  | 'rocky'        // Slows movement, damages over time
  | 'fertile'      // Food spawns here more often
  | 'barren'       // No food spawns
  | 'nest'         // Safe zone, energy regeneration bonus;

// Spatial partitioning for performance
const worldGrid: WorldGrid = {
  width: 1600,
  height: 1200,
  cellSize: 50,
  cells: new Map()
};

function getCellKey(x: number, y: number): string {
  const cellX = Math.floor(x / worldGrid.cellSize);
  const cellY = Math.floor(y / worldGrid.cellSize);
  return `${cellX},${cellY}`;
}

function getCell(position: { x: number; y: number }): GridCell {
  const key = getCellKey(position.x, position.y);
  return worldGrid.cells.get(key) || createEmptyCell(position);
}
```

### Spatial Queries (Like Warcraft)

```typescript
// All interactions use spatial queries
interface SpatialQuery {
  // Basic proximity
  findInRadius(center: Vector2, radius: number, filter?: EntityFilter): Entity[];
  findInRectangle(topLeft: Vector2, bottomRight: Vector2, filter?: EntityFilter): Entity[];

  // Line of sight (Warcraft fog of war)
  hasLineOfSight(from: Vector2, to: Vector2): boolean;
  getVisibleArea(center: Vector2, radius: number): GridCell[];

  // Terrain-based
  getTerrainAt(position: Vector2): TerrainType;
  isPassable(position: Vector2): boolean;
  getMovementSpeed(position: Vector2, baseSpeed: number): number;

  // Resource discovery
  findNearestResource(position: Vector2, type: ResourceType, maxDistance: number): Resource | null;

  // Threat detection
  findThreats(position: Vector2, radius: number): Hazard[];
}
```

---

## DYNAMIC ENVIRONMENT

### Environmental State Changes

The world **changes over time** in response to Thronglet actions and natural cycles:

```typescript
interface EnvironmentalState {
  // Time of day cycle (affects visibility, energy)
  timeOfDay: 'dawn' | 'day' | 'dusk' | 'night';
  dayNightProgress: number; // 0-1

  // Weather system
  weather: 'clear' | 'rain' | 'storm' | 'drought';
  weatherIntensity: number; // 0-1

  // Seasonal cycle
  season: 'spring' | 'summer' | 'autumn' | 'winter';
  seasonProgress: number; // 0-1

  // Resource availability
  foodAbundance: number; // 0-1 (global food spawn rate)

  // Population pressure
  throngletDensity: number; // Total Thronglets / world area

  // Hazards
  activeHazards: Hazard[];
}

// Environment changes over time
function updateEnvironment(deltaTime: number): void {
  // Day/night cycle (1 full cycle = 300 seconds = 5 minutes)
  environmentalState.dayNightProgress += deltaTime / 300;
  if (environmentalState.dayNightProgress >= 1.0) {
    environmentalState.dayNightProgress = 0;
  }

  // Update time of day
  const progress = environmentalState.dayNightProgress;
  if (progress < 0.25) {
    environmentalState.timeOfDay = 'dawn';
  } else if (progress < 0.5) {
    environmentalState.timeOfDay = 'day';
  } else if (progress < 0.75) {
    environmentalState.timeOfDay = 'dusk';
  } else {
    environmentalState.timeOfDay = 'night';
  }

  // Weather changes (random events)
  if (Math.random() < 0.001) { // 0.1% chance per tick
    environmentalState.weather = randomWeatherType();
    environmentalState.weatherIntensity = Math.random();
    console.log(`🌦️  Weather changed to ${environmentalState.weather} (${environmentalState.weatherIntensity})`);
  }

  // Seasonal progression (1 season = 600 seconds = 10 minutes)
  environmentalState.seasonProgress += deltaTime / 600;
  if (environmentalState.seasonProgress >= 1.0) {
    environmentalState.seasonProgress = 0;
    environmentalState.season = nextSeason(environmentalState.season);
    console.log(`🍂 Season changed to ${environmentalState.season}`);
  }

  // Food abundance based on season
  switch (environmentalState.season) {
    case 'spring':
      environmentalState.foodAbundance = 1.2; // +20% food
      break;
    case 'summer':
      environmentalState.foodAbundance = 1.5; // +50% food
      break;
    case 'autumn':
      environmentalState.foodAbundance = 0.9; // -10% food
      break;
    case 'winter':
      environmentalState.foodAbundance = 0.5; // -50% food (scarcity)
      break;
  }

  // Apply environmental effects to all cells
  applyEnvironmentalEffects();
}
```

### Terrain Modification (Like Warcraft Building)

Thronglets can **modify the terrain** through actions:

```typescript
// Thronglets create "nests" when resting in groups
function checkNestFormation(thronglets: Thronglet[]): void {
  // Group detection
  const groups = findStaticGroups(thronglets, {
    minSize: 5,
    minTime: 100, // 10 seconds stationary
    radius: 80
  });

  for (const group of groups) {
    const centerPos = calculateCenterOfMass(group.members);
    const cell = getCell(centerPos);

    // Transform terrain to "nest"
    if (cell.terrain !== 'nest' && cell.terrain === 'grass') {
      cell.terrain = 'nest';
      cell.environmentalEffects.push({
        type: 'energy-regeneration',
        strength: 0.02, // +2% energy/tick
        radius: 80
      });

      console.log(`🏠 Nest formed at (${centerPos.x}, ${centerPos.y})`);

      // Visual feedback
      EventDaemon.broadcast('terrain:nest-formed', {
        position: centerPos,
        creatorIds: group.members.map(t => t.id)
      });
    }
  }
}

// Nests decay over time if abandoned
function updateNests(): void {
  for (const [key, cell] of worldGrid.cells) {
    if (cell.terrain === 'nest') {
      // Check if still occupied
      if (cell.occupants.length < 3) {
        cell.nestAbandonedTime = (cell.nestAbandonedTime || 0) + 1;

        // After 50 ticks (5 seconds) abandoned, revert to grass
        if (cell.nestAbandonedTime > 50) {
          cell.terrain = 'grass';
          cell.environmentalEffects = cell.environmentalEffects.filter(
            e => e.type !== 'energy-regeneration'
          );
          console.log(`🏚️  Nest at (${cell.position.x}, ${cell.position.y}) decayed`);
        }
      } else {
        cell.nestAbandonedTime = 0; // Reset decay timer
      }
    }
  }
}
```

### Resource Depletion and Regeneration

```typescript
interface Resource {
  id: string;
  type: ResourceType;
  position: Vector2;
  energy: number; // Amount of energy available
  maxEnergy: number;
  regenerationRate: number; // Energy/tick
  depleted: boolean;
}

type ResourceType = 'food' | 'water' | 'energy-crystal';

function updateResources(deltaTime: number): void {
  for (const resource of gameState.resources) {
    // Regenerate depleted resources over time
    if (resource.energy < resource.maxEnergy) {
      resource.energy += resource.regenerationRate * deltaTime;
      resource.energy = Math.min(resource.energy, resource.maxEnergy);

      if (resource.depleted && resource.energy > 0) {
        resource.depleted = false;
        console.log(`♻️  Resource ${resource.id} regenerated`);
      }
    }

    // Mark as depleted if empty
    if (resource.energy <= 0) {
      resource.depleted = true;
    }

    // Seasonal modifiers
    const seasonalMultiplier = environmentalState.foodAbundance;
    resource.regenerationRate = resource.baseRegenerationRate * seasonalMultiplier;
  }
}

// Thronglet consumes resource
function consumeResource(thronglet: Thronglet, resource: Resource): number {
  if (resource.depleted || resource.energy <= 0) {
    return 0;
  }

  // Consume based on Thronglet's needs
  const needed = thronglet.genome.physical.maxEnergy - thronglet.energy;
  const consumed = Math.min(needed, resource.energy, 0.3); // Max 0.3 energy per interaction

  resource.energy -= consumed;
  thronglet.energy += consumed;

  console.log(`🍴 ${thronglet.id} consumed ${consumed.toFixed(2)} from resource ${resource.id}`);

  return consumed;
}
```

---

## RTS-STYLE INTERACTION RULES

### Proximity-Based Interactions (Like Warcraft Units)

```typescript
// All interactions require spatial proximity
interface InteractionRules {
  // Communication (already covered, but enforced by proximity)
  communicationRange: number; // 150 pixels

  // Resource gathering
  gatherRange: number; // 30 pixels (must be close to food)

  // Reproduction
  reproductionRange: number; // 100 pixels (partner must be nearby)

  // Social influence
  flockingRange: number; // 100 pixels (movement coordination)
  leadershipRange: number; // 200 pixels (leaders influence wider area)

  // Threat detection
  hazardDetectionRange: number; // 150 pixels (sense danger)

  // Collision
  collisionRadius: number; // 15 pixels (physical size)
}

const INTERACTION_RULES: InteractionRules = {
  communicationRange: 150,
  gatherRange: 30,
  reproductionRange: 100,
  flockingRange: 100,
  leadershipRange: 200,
  hazardDetectionRange: 150,
  collisionRadius: 15
};

// Enforce interaction rules every tick
function enforceInteractionRules(thronglet: Thronglet): void {
  const cell = getCell(thronglet.position);

  // 1. Terrain effects
  applyTerrainEffects(thronglet, cell);

  // 2. Environmental effects
  applyEnvironmentalEffects(thronglet, cell);

  // 3. Resource gathering (must be in range)
  const nearbyResources = findInRadius(
    thronglet.position,
    INTERACTION_RULES.gatherRange,
    { type: 'resource' }
  );

  if (nearbyResources.length > 0 && thronglet.energy < 0.9) {
    const resource = nearbyResources[0] as Resource;
    consumeResource(thronglet, resource);
  }

  // 4. Communication (must be in range)
  const nearbyThronglets = findInRadius(
    thronglet.position,
    INTERACTION_RULES.communicationRange,
    { type: 'thronglet' }
  );

  if (nearbyThronglets.length > 0) {
    processProximityCommunication(thronglet, nearbyThronglets);
  }

  // 5. Hazard detection
  const nearbyHazards = findThreats(
    thronglet.position,
    INTERACTION_RULES.hazardDetectionRange
  );

  if (nearbyHazards.length > 0) {
    reactToHazards(thronglet, nearbyHazards);
  }

  // 6. Collision avoidance
  const tooClose = findInRadius(
    thronglet.position,
    INTERACTION_RULES.collisionRadius,
    { type: 'thronglet', exclude: thronglet.id }
  );

  if (tooClose.length > 0) {
    applyCollisionAvoidance(thronglet, tooClose);
  }
}
```

### Line of Sight System (Warcraft Fog of War)

```typescript
// Thronglets can only "see" what's in their line of sight
function updateVisibility(thronglet: Thronglet): void {
  const visionRadius = thronglet.genome.physical.senseRadius * 200; // Base 200 pixels

  // Get all cells in vision radius
  const visibleCells = getVisibleArea(thronglet.position, visionRadius);

  for (const cell of visibleCells) {
    // Check line of sight (forests block vision)
    if (hasLineOfSight(thronglet.position, cell.position)) {
      cell.visibility = 1.0; // Fully visible

      // Thronglet can now "see" entities in this cell
      for (const entityId of cell.occupants) {
        thronglet.knownEntities.add(entityId);
      }

      // Discover resources
      for (const resource of cell.resources) {
        if (!thronglet.knownResources.has(resource.id)) {
          thronglet.knownResources.set(resource.id, resource);
          console.log(`👁️  ${thronglet.id} discovered resource at (${resource.position.x}, ${resource.position.y})`);
        }
      }
    } else {
      cell.visibility = 0.3; // Partially obscured
    }
  }
}

function hasLineOfSight(from: Vector2, to: Vector2): boolean {
  // Raycast between positions
  const steps = 20;
  const dx = (to.x - from.x) / steps;
  const dy = (to.y - from.y) / steps;

  for (let i = 0; i < steps; i++) {
    const checkPos = {
      x: from.x + dx * i,
      y: from.y + dy * i
    };

    const cell = getCell(checkPos);

    // Forests and rocky terrain block line of sight
    if (cell.terrain === 'forest' || cell.terrain === 'rocky') {
      return false;
    }
  }

  return true;
}
```

---

## ENVIRONMENTAL RESPONSE BEHAVIORS

### Thronglets React to Environmental Changes

```typescript
// AI decisions now include environmental context
interface ThrongletDecisionContext {
  // Standard context
  energy: number;
  nearbyThronglets: number;
  nearbyFood: number;

  // ADDED: Environmental context
  currentTerrain: TerrainType;
  timeOfDay: string;
  weather: string;
  weatherIntensity: number;
  isInDanger: boolean;
  nearbyHazards: Hazard[];
  environmentalEffects: EnvironmentalEffect[];
  knownResources: Resource[];
}

// Example: Night behavior
async function makeDecision(thronglet: Thronglet): Promise<ThrongletAction> {
  const context = buildDecisionContext(thronglet);

  // Night time → seek nest or rest
  if (context.timeOfDay === 'night') {
    // High-fearfulness Thronglets strongly prefer nests at night
    if (thronglet.genome.behavioral.fearfulness > 0.6) {
      const nearestNest = findNearestTerrain(thronglet.position, 'nest', 500);

      if (nearestNest) {
        return {
          action: 'move-toward',
          target: nearestNest.position,
          reasoning: 'Seeking safety of nest during night'
        };
      } else if (context.nearbyThronglets >= 3) {
        // No nest nearby, huddle with others
        return {
          action: 'rest',
          reasoning: 'Huddling with group for safety during night'
        };
      }
    }
  }

  // Storm → seek shelter
  if (context.weather === 'storm' && context.weatherIntensity > 0.7) {
    const shelter = findNearestTerrain(thronglet.position, 'forest', 300);

    if (shelter) {
      return {
        action: 'move-toward',
        target: shelter.position,
        reasoning: 'Seeking forest cover from storm'
      };
    }
  }

  // Rocky terrain → move away (unless desperate for food)
  if (context.currentTerrain === 'rocky' && thronglet.energy > 0.3) {
    const nearbyGrass = findNearestTerrain(thronglet.position, 'grass', 200);

    if (nearbyGrass) {
      return {
        action: 'move-toward',
        target: nearbyGrass.position,
        reasoning: 'Rocky terrain is damaging, seeking grass'
      };
    }
  }

  // Winter scarcity → aggressive food seeking
  if (context.season === 'winter' && thronglet.energy < 0.5) {
    // More willing to compete for resources
    const knownFood = Array.from(thronglet.knownResources.values())
      .filter(r => r.type === 'food' && !r.depleted);

    if (knownFood.length > 0) {
      const nearest = findNearest(thronglet.position, knownFood);

      return {
        action: 'move-toward',
        target: nearest.position,
        urgency: 'high',
        reasoning: 'Winter scarcity - must reach food quickly'
      };
    }
  }

  // Continue with standard decision logic...
  return standardDecisionLogic(thronglet, context);
}
```

### Hazard System

```typescript
interface Hazard {
  id: string;
  type: HazardType;
  position: Vector2;
  radius: number;
  damage: number; // Energy loss per tick
  duration: number; // Ticks until hazard dissipates
  createdAt: number;
}

type HazardType =
  | 'fire'        // Spreads to nearby cells
  | 'poison'      // Lingers, slow damage
  | 'predator'    // Moves toward Thronglets
  | 'flood'       // Fills low-lying areas;

// Spawn hazards dynamically
function spawnRandomHazards(): void {
  // 0.5% chance per tick
  if (Math.random() < 0.005) {
    const hazardType = randomHazardType();
    const position = randomWorldPosition();

    const hazard: Hazard = {
      id: generateId(),
      type: hazardType,
      position,
      radius: 80,
      damage: 0.05, // 5% energy per tick
      duration: 200, // 20 seconds
      createdAt: gameState.tick
    };

    gameState.hazards.push(hazard);

    console.log(`⚠️  ${hazardType} hazard spawned at (${position.x}, ${position.y})`);

    EventDaemon.broadcast('hazard:spawned', { hazard });
  }
}

// Update hazards (spread, move, decay)
function updateHazards(): void {
  for (let i = gameState.hazards.length - 1; i >= 0; i--) {
    const hazard = gameState.hazards[i];

    // Decay duration
    hazard.duration--;

    if (hazard.duration <= 0) {
      // Hazard dissipates
      gameState.hazards.splice(i, 1);
      console.log(`✅ ${hazard.type} hazard dissipated`);
      EventDaemon.broadcast('hazard:dissipated', { hazardId: hazard.id });
      continue;
    }

    // Special behaviors per hazard type
    switch (hazard.type) {
      case 'fire':
        // Fire spreads to adjacent cells
        if (Math.random() < 0.1) { // 10% chance per tick
          const spreadPos = {
            x: hazard.position.x + (Math.random() - 0.5) * 100,
            y: hazard.position.y + (Math.random() - 0.5) * 100
          };

          spawnHazard('fire', spreadPos, hazard.duration - 50);
        }
        break;

      case 'predator':
        // Predator moves toward nearest Thronglet
        const nearest = findNearestThronglet(hazard.position);
        if (nearest) {
          const direction = normalize({
            x: nearest.position.x - hazard.position.x,
            y: nearest.position.y - hazard.position.y
          });

          hazard.position.x += direction.x * 50; // 50 pixels per tick
          hazard.position.y += direction.y * 50;
        }
        break;
    }

    // Apply damage to Thronglets in range
    const victimsInRange = findInRadius(
      hazard.position,
      hazard.radius,
      { type: 'thronglet' }
    );

    for (const victim of victimsInRange as Thronglet[]) {
      victim.energy -= hazard.damage;

      if (victim.energy <= 0) {
        handleThrongletDeath(victim, `killed by ${hazard.type}`);
      }
    }
  }
}

// Thronglets react to hazards
function reactToHazards(thronglet: Thronglet, hazards: Hazard[]): void {
  // Fearfulness determines reaction intensity
  const fearLevel = thronglet.genome.behavioral.fearfulness;

  for (const hazard of hazards) {
    const distance = calculateDistance(thronglet.position, hazard.position);

    // If too close, flee immediately
    if (distance < hazard.radius * 1.5) {
      const fleeDirection = normalize({
        x: thronglet.position.x - hazard.position.x,
        y: thronglet.position.y - hazard.position.y
      });

      thronglet.velocity = {
        x: fleeDirection.x * thronglet.genome.physical.maxSpeed * 1.5, // Panic boost
        y: fleeDirection.y * thronglet.genome.physical.maxSpeed * 1.5
      };

      thronglet.motivation = 'fleeing-danger';

      // Warn others (if chatty)
      if (Math.random() < thronglet.genome.communication.chattiness * fearLevel) {
        sendMessage(thronglet, 'danger', { position: hazard.position });
      }
    }
  }
}
```

---

## VISUAL FEEDBACK SYSTEM

### Real-Time Spatial Visualization (Like Warcraft)

```typescript
// Three.js rendering with spatial state indicators
function renderSpatialState(scene: THREE.Scene): void {
  // 1. Terrain visualization
  for (const [key, cell] of worldGrid.cells) {
    const material = getTerrainMaterial(cell.terrain);
    const geometry = new THREE.PlaneGeometry(
      worldGrid.cellSize,
      worldGrid.cellSize
    );
    const mesh = new THREE.Mesh(geometry, material);
    mesh.position.set(cell.position.x, 0, cell.position.y);
    mesh.rotation.x = -Math.PI / 2;
    scene.add(mesh);

    // Overlay effects (nests glow, hazards pulse)
    if (cell.terrain === 'nest') {
      addGlowEffect(mesh, 0x00ff00);
    }
  }

  // 2. Interaction range indicators (when selected)
  if (selectedThronglet) {
    // Communication range
    drawCircle(scene, selectedThronglet.position, INTERACTION_RULES.communicationRange, 0x4444ff, 0.2);

    // Vision range
    drawCircle(scene, selectedThronglet.position, selectedThronglet.genome.physical.senseRadius * 200, 0xffff00, 0.1);

    // Danger detection range
    drawCircle(scene, selectedThronglet.position, INTERACTION_RULES.hazardDetectionRange, 0xff0000, 0.15);
  }

  // 3. Resource states (full vs depleted)
  for (const resource of gameState.resources) {
    const color = resource.depleted ? 0x888888 : 0x00ff00;
    const size = resource.energy / resource.maxEnergy; // Shrinks as depleted

    drawResource(scene, resource.position, size, color);
  }

  // 4. Hazard visualization
  for (const hazard of gameState.hazards) {
    const color = getHazardColor(hazard.type);
    const opacity = hazard.duration / 200; // Fades as it decays

    drawHazard(scene, hazard.position, hazard.radius, color, opacity);
  }

  // 5. Communication lines (active messages)
  for (const message of activeMessages) {
    drawMessageLine(scene, message.senderId, message.recipientIds, message.urgency);
  }

  // 6. Environmental effects (rain particles, day/night lighting)
  updateEnvironmentalVisuals(scene, environmentalState);
}

function getTerrainMaterial(terrain: TerrainType): THREE.MeshStandardMaterial {
  const colors = {
    grass: 0x44aa44,
    forest: 0x226622,
    water: 0x4444ff,
    rocky: 0x888888,
    fertile: 0x66cc66,
    barren: 0xaa8844,
    nest: 0xffcc44
  };

  return new THREE.MeshStandardMaterial({
    color: colors[terrain],
    roughness: 0.8,
    metalness: 0.2
  });
}

function drawCircle(
  scene: THREE.Scene,
  center: Vector2,
  radius: number,
  color: number,
  opacity: number
): void {
  const geometry = new THREE.RingGeometry(radius - 2, radius, 32);
  const material = new THREE.MeshBasicMaterial({
    color,
    transparent: true,
    opacity,
    side: THREE.DoubleSide
  });
  const circle = new THREE.Mesh(geometry, material);
  circle.position.set(center.x, 0.1, center.y);
  circle.rotation.x = -Math.PI / 2;
  scene.add(circle);
}
```

### UI Overlays (Warcraft-Style Information)

```typescript
// HUD displays spatial state information
interface SpatialHUD {
  // Selected Thronglet info
  selectedThronglet: {
    id: string;
    energy: number;
    genome: ThrongletGenome;
    currentTerrain: TerrainType;
    nearbyResources: number;
    nearbyThronglets: number;
    nearbyHazards: number;
    motivation: string;
  };

  // Environmental info
  environment: {
    timeOfDay: string;
    weather: string;
    season: string;
    foodAbundance: number;
  };

  // Population statistics
  population: {
    total: number;
    births: number;
    deaths: number;
    avgEnergy: number;
    avgGeneration: number;
  };

  // Minimap (like Warcraft)
  minimap: {
    width: number;
    height: number;
    throngletPositions: Vector2[];
    resourcePositions: Vector2[];
    hazardPositions: Vector2[];
  };
}
```

---

## IMPLEMENTATION

### Spatial Rules Recipe

```json
{
  "uniqueId": "thronglets-spatial-rules",
  "name": "Thronglet Spatial Rules Engine",

  "pipeline": [
    {
      "command": "game/update-grid",
      "params": {
        "thronglets": "$gameState.thronglets",
        "resources": "$gameState.resources",
        "hazards": "$gameState.hazards"
      },
      "outputTo": "updatedGrid"
    },

    {
      "command": "game/update-environment",
      "params": {
        "deltaTime": 0.1
      },
      "outputTo": "environmentalState"
    },

    {
      "command": "game/apply-terrain-effects",
      "params": {
        "thronglets": "$gameState.thronglets",
        "grid": "$updatedGrid"
      }
    },

    {
      "command": "game/update-visibility",
      "params": {
        "thronglets": "$gameState.thronglets",
        "grid": "$updatedGrid"
      }
    },

    {
      "command": "game/enforce-interaction-rules",
      "params": {
        "thronglets": "$gameState.thronglets",
        "rules": "$INTERACTION_RULES"
      }
    },

    {
      "command": "game/update-hazards",
      "params": {
        "hazards": "$gameState.hazards",
        "thronglets": "$gameState.thronglets"
      },
      "outputTo": "updatedHazards"
    },

    {
      "command": "game/check-terrain-modification",
      "params": {
        "thronglets": "$gameState.thronglets",
        "grid": "$updatedGrid"
      }
    },

    {
      "command": "game/render-spatial-state",
      "params": {
        "grid": "$updatedGrid",
        "environmentalState": "$environmentalState",
        "selectedThronglet": "$selectedThronglet"
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

By implementing a **spatial rules engine** like Warcraft/RTS games, Thronglets becomes a **dynamic, reactive ecosystem** where:

### Spatial State Dictates Everything:
- **Proximity** enables communication, reproduction, resource gathering
- **Terrain type** affects movement speed, vision, energy
- **Line of sight** creates fog of war exploration gameplay
- **Environmental state** (weather, time of day, season) changes behavior

### Dynamic Environment Creates Emergence:
- **Day/night cycles** shift Thronglet activity patterns
- **Weather events** force shelter-seeking behavior
- **Seasonal scarcity** drives migration and competition
- **Terrain modification** (nest building) changes the world
- **Hazards** create threats Thronglets must avoid or flee

### Visual Feedback Like Warcraft:
- **Interaction range circles** show communication zones
- **Terrain coloring** indicates passability and effects
- **Resource states** (full vs depleted) visible
- **Hazard pulses** warn of danger
- **Minimap** for strategic overview

### Result: A Living, Breathing World

Players experience Thronglets as a **spatial ecosystem** where positioning, environment, and terrain matter. Like Warcraft, **where you are determines what you can do** - and the world changes in response to Thronglet actions and natural cycles.

Thronglets aren't just AI creatures moving randomly - they're **agents responding to spatial state**, creating emergent patterns like migrations during winter, nest clustering during night, and shelter-seeking during storms.

**Powered by**: Continuum's recipe system orchestrating spatial rules, PersonaUser AI making environmentally-aware decisions, and Three.js rendering real-time spatial state visualization.
