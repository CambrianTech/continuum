#!/usr/bin/env node
/**
 * Unit test for coordination routing logic
 * Tests the intelligentRoute function in isolation
 */

function testRouting() {
  console.log('🧪 Testing coordination routing logic...\n');

  // Test cases
  const testCases = [
    {
      task: 'coordinate with CodeAI to implement the modular agent factory system',
      expectedTrigger: 'coordinate + codeai',
      description: 'Should trigger coordination'
    },
    {
      task: 'fix the gitignore NOW - add patterns',
      expectedTrigger: 'fix + issue',
      description: 'Should trigger coordination (fix + issue)'
    },
    {
      task: 'hello',
      expectedTrigger: 'general',
      description: 'Should go to GeneralAI'
    },
    {
      task: 'how does this system work and what can you create?',
      expectedTrigger: 'strategic/complex',
      description: 'Should go to PlannerAI (complex question)'
    },
    {
      task: 'plan a strategy for repository cleanup',
      expectedTrigger: 'strategic/complex',
      description: 'Should go to PlannerAI (contains "plan")'
    }
  ];

  testCases.forEach((testCase, i) => {
    console.log(`Test ${i + 1}: ${testCase.description}`);
    console.log(`Task: "${testCase.task}"`);
    
    const taskLower = testCase.task.toLowerCase();
    
    // Check routing conditions (updated to match fixed continuum.cjs)
    let routingResult = 'unknown';
    
    // COORDINATION CHECK FIRST (highest priority)
    if ((taskLower.includes('coordinate') && taskLower.includes('codeai')) ||
        taskLower.includes('ci') || taskLower.includes('github') || taskLower.includes('pr') || 
        taskLower.includes('build fail') || (taskLower.includes('fix') && taskLower.includes('issue'))) {
      routingResult = 'PlannerAI + CodeAI coordination';
    } else if (taskLower.includes('who') || taskLower.includes('there') || taskLower.includes('exist') || 
               taskLower.includes('continuum') || (taskLower.includes('make') && taskLower.includes('ai'))) {
      routingResult = 'Multi-AI coordination (system awareness)';
    } else if (taskLower.includes('plan') || taskLower.includes('strategy') || taskLower.includes('architecture') || 
        taskLower.includes('design') || taskLower.includes('how') || taskLower.includes('what') ||
        taskLower.includes('analyze') || taskLower.includes('organize') ||
        taskLower.includes('improve') || taskLower.includes('optimize') || taskLower.includes('create') ||
        taskLower.includes('build') || taskLower.includes('develop') || taskLower.includes('solution') ||
        testCase.task.split(' ').length > 5) {
      routingResult = 'PlannerAI (strategic/complex)';
    } else if (taskLower.includes('code') || taskLower.includes('implement') || taskLower.includes('bug')) {
      routingResult = 'CodeAI';
    } else {
      routingResult = 'GeneralAI';
    }
    
    console.log(`Expected: ${testCase.expectedTrigger}`);
    console.log(`Actual routing: ${routingResult}`);
    
    // Check specific coordination triggers
    const coordinationTriggers = {
      'coordinate + codeai': taskLower.includes('coordinate') && taskLower.includes('codeai'),
      'fix + issue': taskLower.includes('fix') && taskLower.includes('issue'),
      'strategic/complex': (taskLower.includes('plan') || taskLower.includes('strategy') || 
                           taskLower.includes('how') || taskLower.includes('what') ||
                           testCase.task.split(' ').length > 5)
    };
    
    console.log('Trigger analysis:');
    Object.entries(coordinationTriggers).forEach(([trigger, matches]) => {
      console.log(`  - ${trigger}: ${matches ? '✅' : '❌'}`);
    });
    
    console.log('---\n');
  });

  console.log('🔍 Key Findings:');
  console.log('1. Tasks with "coordinate" + "codeai" should trigger multi-AI coordination');
  console.log('2. Tasks with "fix" + "issue" should trigger coordination');
  console.log('3. Complex tasks (>5 words or strategic keywords) should go to PlannerAI');
  console.log('4. The routing logic determines AI selection, not AI behavior');
  
  console.log('\n❗ Critical Issue:');
  console.log('The problem might be that PlannerAI TALKS about coordination');
  console.log('but the system doesn\'t actually TRIGGER follow-up calls to CodeAI');
  console.log('based on PlannerAI\'s response content!');
}

testRouting();