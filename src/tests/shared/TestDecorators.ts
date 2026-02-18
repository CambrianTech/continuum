/**
 * Middle-Out Test Classification System
 * Decorators for organizing tests by architectural layer and importance
 */

// Import well-defined types
export { 
  TestLevel, 
  TestImportance, 
  TestCategory, 
  TestProfile,
  TestMetadata,
  TestRunConfig,
  TestResult,
  TestProfileConfig,
  TestClassificationGuards,
  IMPORTANCE_ORDER,
  CATEGORY_CRITICALITY
} from './TestClassificationTypes';


// Test registry to track classified tests
const testRegistry: Map<string, TestMetadata> = new Map();

/**
 * Decorator to classify tests in the middle-out architecture
 */
export function TestSpec(metadata: TestMetadata) {
  return function(target: any, propertyKey?: string, descriptor?: PropertyDescriptor) {
    const testName = propertyKey || target.name || 'unknown';
    testRegistry.set(testName, metadata);
    
    // Add metadata to function for runtime access
    if (descriptor && descriptor.value) {
      descriptor.value._testMetadata = metadata;
    } else if (target) {
      target._testMetadata = metadata;
    }
    
    return descriptor || target;
  };
}

/**
 * Get test metadata by name
 */
export function getTestMetadata(testName: string): TestMetadata | undefined {
  return testRegistry.get(testName);
}

/**
 * Get all tests by criteria
 */
export function getTestsByLevel(level: TestLevel): string[] {
  return Array.from(testRegistry.entries())
    .filter(([_, metadata]) => metadata.level === level)
    .map(([name, _]) => name);
}

export function getTestsByImportance(importance: TestImportance): string[] {
  return Array.from(testRegistry.entries())
    .filter(([_, metadata]) => metadata.importance === importance)
    .map(([name, _]) => name);
}

export function getTestsByCategory(category: TestCategory): string[] {
  return Array.from(testRegistry.entries())
    .filter(([_, metadata]) => metadata.category === category)
    .map(([name, _]) => name);
}

/**
 * Generate test execution plan based on middle-out strategy
 */
export function generateExecutionPlan(): { [key in TestLevel]: string[] } {
  const plan: { [key in TestLevel]: string[] } = {
    [TestLevel.FOUNDATION]: [],
    [TestLevel.UNIT]: [],
    [TestLevel.INTEGRATION]: [],
    [TestLevel.SYSTEM]: [],
    [TestLevel.E2E]: []
  };
  
  // Sort by importance within each level
  Object.values(TestLevel).forEach(level => {
    const testsInLevel = getTestsByLevel(level);
    const sortedTests = testsInLevel.sort((a, b) => {
      const metadataA = getTestMetadata(a)!;
      const metadataB = getTestMetadata(b)!;
      
      // Use imported importance ordering
      return IMPORTANCE_ORDER[metadataA.importance] - IMPORTANCE_ORDER[metadataB.importance];
    });
    
    plan[level] = sortedTests;
  });
  
  return plan;
}

/**
 * Export test registry for analysis
 */
export function getAllTests(): Map<string, TestMetadata> {
  return new Map(testRegistry);
}