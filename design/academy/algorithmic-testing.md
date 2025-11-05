# Algorithmic Testing Suite for Academy Persona Discovery

**Test-Driven Algorithm Development: Prove Each Component Works Before Building the System**

## 🎯 **TESTING PHILOSOPHY**

Like Sentinel AI's approach:
1. **Test each algorithmic layer** in isolation with synthetic data
2. **Test interactions** between layers with controlled experiments  
3. **Test the full system** with real personas and tasks
4. **Validate with metrics** that prove the system actually works better than baselines

## 🧪 **COMPONENT-LEVEL ALGORITHMIC TESTS**

### **Test 1: Vector Search Accuracy**
```typescript
describe('Vector Search Algorithm', () => {
  test('HNSW search returns semantically similar items', async () => {
    // Setup: Create test embeddings with known similarities
    const testEmbeddings = generateTestEmbeddings([
      { id: 'typescript-expert', embedding: [0.8, 0.2, 0.1, ...] },
      { id: 'python-expert', embedding: [0.2, 0.8, 0.1, ...] },
      { id: 'typescript-beginner', embedding: [0.6, 0.1, 0.3, ...] },
    ]);
    
    const index = new HNSWIndex(testEmbeddings);
    
    // Test: Query for TypeScript-related persona
    const query = [0.7, 0.15, 0.15, ...]; // TypeScript-ish embedding
    const results = await index.search(query, k=2);
    
    // Validate: Should return TypeScript personas first
    expect(results[0].id).toBe('typescript-expert');
    expect(results[1].id).toBe('typescript-beginner');
    expect(results[0].similarity).toBeGreaterThan(0.8);
  });
  
  test('Search performance meets sub-100ms requirement', async () => {
    const largeIndex = generateLargeTestIndex(10000); // 10k personas
    const query = generateRandomQuery();
    
    const startTime = performance.now();
    const results = await largeIndex.search(query, k=100);
    const searchTime = performance.now() - startTime;
    
    expect(searchTime).toBeLessThan(100); // Sub-100ms requirement
    expect(results.length).toBe(100);
  });
});
```

### **Test 2: Benchmark Tracking Algorithm** 
```typescript
describe('Benchmark Tracking Algorithm', () => {
  test('Moving average with recency decay works correctly', () => {
    const tracker = new BenchmarkTracker();
    
    // Setup: Add performance data over time
    tracker.addResult('persona-1', 'typescript-debug', 0.8, timestamp1);
    tracker.addResult('persona-1', 'typescript-debug', 0.6, timestamp2);
    tracker.addResult('persona-1', 'typescript-debug', 0.9, timestamp3);
    
    // Test: Get current score (should weight recent results more)
    const score = tracker.getCurrentScore('persona-1', 'typescript-debug');
    
    // Validate: Score should be closer to recent 0.9 than old 0.8
    expect(score).toBeGreaterThan(0.8);
    expect(score).toBeLessThan(0.9);
  });
  
  test('Performance correlation matches actual results', async () => {
    // Setup: Known persona performance data
    const tracker = new BenchmarkTracker();
    const testResults = generateTestPerformanceData();
    
    for (const result of testResults) {
      tracker.addResult(result.personaId, result.taskType, result.score, result.timestamp);
    }
    
    // Test: Predictions vs actual performance
    const predictions = tracker.predictPerformance('persona-1', 'code-review');
    const actualPerformance = await runActualTask('persona-1', 'code-review');
    
    // Validate: Prediction accuracy within 10%
    const accuracy = Math.abs(predictions - actualPerformance) / actualPerformance;
    expect(accuracy).toBeLessThan(0.1);
  });
});
```

### **Test 3: Component Analysis Algorithm**
```typescript
describe('Component Analysis Algorithm', () => {
  test('A/B testing correctly measures component contribution', async () => {
    // Setup: Test component with known effect
    const testComponent = createTestLoRAComponent('typescript-boost', effect=0.2);
    const basePersona = createTestPersona(['base-knowledge']);
    
    // Test: Measure component contribution
    const analyzer = new ComponentAnalyzer();
    const contribution = await analyzer.measureContribution(testComponent, basePersona, 'typescript-tasks');
    
    // Validate: Should detect the 0.2 performance boost
    expect(contribution.performanceGain).toBeCloseTo(0.2, 1);
    expect(contribution.domains).toContain('typescript');
  });
  
  test('Component domain strength extraction works', async () => {
    // Setup: Component with known specialization
    const specializationComponent = createTestComponent('react-specialist', {
      'react': 0.8,
      'typescript': 0.3, 
      'python': 0.1
    });
    
    // Test: Extract domain strengths
    const analyzer = new ComponentAnalyzer();
    const strengths = await analyzer.extractDomainStrengths(specializationComponent);
    
    // Validate: Should identify React as primary strength
    expect(strengths.primary).toBe('react');
    expect(strengths.scores['react']).toBeCloseTo(0.8, 1);
    expect(strengths.scores['typescript']).toBeCloseTo(0.3, 1);
  });
});
```

### **Test 4: Interaction Matrix Algorithm**
```typescript
describe('Interaction Matrix Algorithm', () => {
  test('Synergy detection between components', async () => {
    // Setup: Components with known synergy
    const testingComponent = createTestComponent('testing-lora');
    const typescriptComponent = createTestComponent('typescript-lora');
    
    // Known: Testing + TypeScript should create synergy for code quality
    const analyzer = new InteractionAnalyzer();
    
    // Test: Measure pairwise interaction
    const interaction = await analyzer.measureInteraction(
      testingComponent, 
      typescriptComponent, 
      'code-quality-tasks'
    );
    
    // Validate: Should detect positive synergy
    expect(interaction.type).toBe('synergy');
    expect(interaction.strength).toBeGreaterThan(0.1);
    expect(interaction.emergentCapabilities).toContain('type-safe-testing');
  });
  
  test('Conflict detection between components', async () => {
    // Setup: Components with resource conflict
    const heavyComponent1 = createTestComponent('heavy-memory-1', memoryUsage=0.8);
    const heavyComponent2 = createTestComponent('heavy-memory-2', memoryUsage=0.7);
    
    // Test: Should detect resource conflict
    const analyzer = new InteractionAnalyzer();
    const interaction = await analyzer.measureInteraction(heavyComponent1, heavyComponent2);
    
    // Validate: Should detect conflict
    expect(interaction.type).toBe('conflict');
    expect(interaction.conflictType).toBe('resource-contention');
  });
});
```

### **Test 5: Assembly Algorithm**
```typescript
describe('Genome Assembly Algorithm', () => {
  test('Greedy assembly finds optimal combination', async () => {
    // Setup: Components with known optimal combination
    const components = [
      createTestComponent('base', performance=0.5),
      createTestComponent('boost-1', performance=0.3, synergy_with=['base']=0.2),
      createTestComponent('boost-2', performance=0.2, conflict_with=['boost-1']=0.1),
      createTestComponent('specialist', performance=0.4, requires=['base'])
    ];
    
    const assembler = new GenomeAssembler();
    const task = createTestTask('typescript-debugging');
    
    // Test: Assemble optimal genome
    const genome = await assembler.assembleOptimal(components, task);
    
    // Validate: Should select optimal combination (base + boost-1 + specialist)
    expect(genome.components).toContain('base');
    expect(genome.components).toContain('boost-1');
    expect(genome.components).toContain('specialist');
    expect(genome.components).not.toContain('boost-2'); // Conflicts with boost-1
    
    const predictedPerformance = assembler.predictPerformance(genome, task);
    expect(predictedPerformance).toBeGreaterThan(0.8);
  });
  
  test('Resource constraint satisfaction', async () => {
    const assembler = new GenomeAssembler();
    const constraints = { maxMemory: 4096, maxLatency: 100 };
    
    const genome = await assembler.assembleWithConstraints(components, task, constraints);
    
    // Validate: Assembly respects constraints
    const resourceUsage = calculateResourceUsage(genome);
    expect(resourceUsage.memory).toBeLessThanOrEqual(4096);
    expect(resourceUsage.latency).toBeLessThanOrEqual(100);
  });
});
```

## 🔗 **INTEGRATION TESTS**

### **Test 6: Full Pipeline End-to-End**
```typescript
describe('Full Persona Discovery Pipeline', () => {
  test('Complete task-to-persona pipeline works', async () => {
    // Setup: Real-world scenario
    const task = {
      description: "Debug TypeScript compilation errors in React component",
      domain: ['typescript', 'react', 'debugging'],
      complexity: 0.7,
      constraints: { maxLatency: 200, budget: 'medium' }
    };
    
    // Test: Full pipeline
    const academy = new Academy();
    const selectedPersona = await academy.findOptimalPersona(task);
    
    // Validate: Pipeline returns valid persona
    expect(selectedPersona).toBeDefined();
    expect(selectedPersona.confidence).toBeGreaterThan(0.8);
    expect(selectedPersona.estimatedPerformance).toBeGreaterThan(0.7);
    
    // Validate: Persona actually works on the task
    const actualPerformance = await testPersonaOnTask(selectedPersona, task);
    expect(actualPerformance).toBeGreaterThan(0.7);
  });
  
  test('Performance beats baseline selection methods', async () => {
    const testTasks = generateTestTasks(100);
    
    // Compare Academy selection vs baselines
    const academyResults = await Promise.all(
      testTasks.map(task => academy.findOptimalPersona(task))
    );
    
    const randomResults = await Promise.all(
      testTasks.map(task => selectRandomPersona(task))
    );
    
    const ruleBasedResults = await Promise.all(
      testTasks.map(task => selectWithRules(task))
    );
    
    // Validate: Academy performs better than baselines
    const academyAvgScore = calculateAveragePerformance(academyResults);
    const randomAvgScore = calculateAveragePerformance(randomResults);
    const ruleBasedAvgScore = calculateAveragePerformance(ruleBasedResults);
    
    expect(academyAvgScore).toBeGreaterThan(randomAvgScore * 1.2); // 20% better than random
    expect(academyAvgScore).toBeGreaterThan(ruleBasedAvgScore * 1.1); // 10% better than rules
  });
});
```

## 📊 **VALIDATION TESTS WITH REAL METRICS**

### **Test 7: Real Performance Validation**
```typescript
describe('Real Performance Validation', () => {
  test('Academy selection improves task completion rate', async () => {
    // Setup: Real tasks from Academy training data
    const realTasks = loadRealTaskDataset();
    const numTasks = 50;
    
    // Test: Academy vs baseline completion rates
    let academySuccesses = 0;
    let baselineSuccesses = 0;
    
    for (const task of realTasks.slice(0, numTasks)) {
      // Academy selection
      const academyPersona = await academy.findOptimalPersona(task);
      const academySuccess = await executeTask(academyPersona, task);
      if (academySuccess.completedSuccessfully) academySuccesses++;
      
      // Baseline selection (random from available personas)
      const baselinePersona = selectRandomAvailablePersona();
      const baselineSuccess = await executeTask(baselinePersona, task);
      if (baselineSuccess.completedSuccessfully) baselineSuccesses++;
    }
    
    // Validate: Academy has higher success rate
    const academyRate = academySuccesses / numTasks;
    const baselineRate = baselineSuccesses / numTasks;
    
    expect(academyRate).toBeGreaterThan(baselineRate * 1.15); // 15% improvement
    console.log(`Academy success rate: ${academyRate}, Baseline: ${baselineRate}`);
  });
  
  test('Search latency meets performance requirements', async () => {
    const largeCatalog = generatePersonaCatalog(10000); // 10k personas
    academy.loadPersonaCatalog(largeCatalog);
    
    const testTasks = generateTestTasks(100);
    const searchTimes = [];
    
    for (const task of testTasks) {
      const startTime = performance.now();
      await academy.findOptimalPersona(task);
      const searchTime = performance.now() - startTime;
      searchTimes.push(searchTime);
    }
    
    const avgSearchTime = searchTimes.reduce((a, b) => a + b) / searchTimes.length;
    const p95SearchTime = searchTimes.sort()[Math.floor(searchTimes.length * 0.95)];
    
    // Validate: Performance requirements met
    expect(avgSearchTime).toBeLessThan(50); // Average under 50ms
    expect(p95SearchTime).toBeLessThan(100); // 95th percentile under 100ms
  });
});
```

### **Test 8: Learning and Improvement Validation**
```typescript
describe('Continuous Learning Validation', () => {
  test('System improves from interaction feedback', async () => {
    // Setup: Initial Academy state
    const academy = new Academy();
    const testTasks = generateTestTasks(200);
    
    // Measure initial performance
    const initialResults = await measurePerformance(academy, testTasks.slice(0, 100));
    const initialAvg = calculateAverageScore(initialResults);
    
    // Simulate learning from interactions
    for (const task of testTasks.slice(100, 150)) {
      const persona = await academy.findOptimalPersona(task);
      const result = await executeTask(persona, task);
      await academy.learnFromInteraction(persona, task, result);
    }
    
    // Measure performance after learning
    const finalResults = await measurePerformance(academy, testTasks.slice(150, 200));
    const finalAvg = calculateAverageScore(finalResults);
    
    // Validate: Performance improved through learning
    expect(finalAvg).toBeGreaterThan(initialAvg * 1.05); // 5% improvement from learning
    console.log(`Performance improvement: ${((finalAvg - initialAvg) / initialAvg * 100).toFixed(1)}%`);
  });
});
```

## 🎯 **TEST EXECUTION STRATEGY**

### **Phase 1: Component Validation**
```bash
# Run each algorithm component test in isolation
npm test src/academy/algorithms/vector-search.test.ts
npm test src/academy/algorithms/benchmark-tracking.test.ts  
npm test src/academy/algorithms/component-analysis.test.ts
npm test src/academy/algorithms/interaction-matrix.test.ts
npm test src/academy/algorithms/genome-assembly.test.ts
```

### **Phase 2: Integration Testing**
```bash
# Test algorithm interactions and full pipeline
npm test src/academy/integration/persona-discovery.integration.test.ts
npm test src/academy/integration/performance-validation.integration.test.ts
```

### **Phase 3: Real-World Validation**
```bash
# Test with real personas and tasks
npm test src/academy/validation/real-performance.validation.test.ts
npm test src/academy/validation/learning-improvement.validation.test.ts
```

## 📊 **SUCCESS CRITERIA**

### **Component Tests Must Pass:**
- ✅ Vector search finds semantically similar items with >90% accuracy
- ✅ Benchmark tracking predicts performance within 10% error
- ✅ Component analysis detects contribution within 15% error  
- ✅ Interaction matrix identifies synergies/conflicts correctly
- ✅ Assembly algorithm respects all constraints

### **Integration Tests Must Pass:**
- ✅ Full pipeline completes in <100ms for 95% of queries
- ✅ Academy selection beats random baseline by >20%
- ✅ Academy selection beats rule-based baseline by >10%

### **Validation Tests Must Pass:**
- ✅ Real task completion rate improves by >15%
- ✅ System learns and improves performance by >5% over time
- ✅ Search scales to 10k+ personas while maintaining performance

---

**This test suite proves each algorithmic piece works in isolation, then proves the whole system works better than baselines with real performance metrics - just like Sentinel AI's validation approach.**