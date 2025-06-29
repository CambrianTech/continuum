/**
 * AI Orchestration Test
 * Tests multiple AIs working together to complete complex tasks
 */

describe('AI Orchestration', () => {
  const baseURL = 'http://localhost:5555';

  test('should orchestrate multiple AIs to build a complete feature', async () => {
    console.log('🎭 Testing AI Orchestration: Building a Todo API');

    // Step 1: PlannerAI creates the architecture
    console.log('📋 Step 1: PlannerAI designing architecture...');
    const planResponse = await fetch(`${baseURL}/ask?role=PlannerAI&task=Design a REST API for a todo app with 4 endpoints: GET all todos, POST new todo, PUT update todo, DELETE todo. Include data models and error handling.`);
    const planResult = await planResponse.json();
    
    expect(planResult.role).toBe('PlannerAI');
    expect(planResult.result).toMatch(/GET|POST|PUT|DELETE/i);
    console.log('✅ PlannerAI created architecture plan');

    // Step 2: CodeAI implements the server
    console.log('💻 Step 2: CodeAI implementing the server...');
    const serverPrompt = `Based on this plan: "${planResult.result.substring(0, 500)}...", implement a Node.js Express server with the todo endpoints. Include middleware for error handling and validation.`;
    const serverResponse = await fetch(`${baseURL}/ask?role=CodeAI&task=${encodeURIComponent(serverPrompt)}`);
    const serverResult = await serverResponse.json();
    
    expect(serverResult.role).toBe('CodeAI');
    expect(serverResult.result).toMatch(/express|app\.get|app\.post/i);
    console.log('✅ CodeAI implemented the server');

    // Step 3: CodeAI creates the database model
    console.log('🗃️ Step 3: CodeAI creating database models...');
    const dbPrompt = `Create a database schema and model for the todo app. Use mongoose/MongoDB with proper validation, indexes, and default values.`;
    const dbResponse = await fetch(`${baseURL}/ask?role=CodeAI&task=${encodeURIComponent(dbPrompt)}`);
    const dbResult = await dbResponse.json();
    
    expect(dbResult.role).toBe('CodeAI');
    expect(dbResult.result).toMatch(/schema|model|mongoose/i);
    console.log('✅ CodeAI created database models');

    // Step 4: PlannerAI creates test strategy 
    console.log('🧪 Step 4: PlannerAI designing test strategy...');
    const testPlanPrompt = `Create a comprehensive testing strategy for the todo API. Include unit tests, integration tests, and API endpoint tests. Specify test cases for success and error scenarios.`;
    const testPlanResponse = await fetch(`${baseURL}/ask?role=PlannerAI&task=${encodeURIComponent(testPlanPrompt)}`);
    const testPlanResult = await testPlanResponse.json();
    
    expect(testPlanResult.role).toBe('PlannerAI');
    expect(testPlanResult.result).toMatch(/test|jest|mocha|chai/i);
    console.log('✅ PlannerAI created test strategy');

    // Step 5: CodeAI implements the tests
    console.log('🔬 Step 5: CodeAI implementing tests...');
    const testCodePrompt = `Based on this test strategy: "${testPlanResult.result.substring(0, 400)}...", implement the actual test code using Jest and Supertest. Include tests for all CRUD operations.`;
    const testCodeResponse = await fetch(`${baseURL}/ask?role=CodeAI&task=${encodeURIComponent(testCodePrompt)}`);
    const testCodeResult = await testCodeResponse.json();
    
    expect(testCodeResult.role).toBe('CodeAI');
    expect(testCodeResult.result).toMatch(/describe|test|expect|supertest/i);
    console.log('✅ CodeAI implemented comprehensive tests');

    // Step 6: GeneralAI reviews and provides feedback
    console.log('👁️ Step 6: GeneralAI reviewing the complete solution...');
    const reviewPrompt = `Review this todo API implementation and provide feedback on architecture, code quality, and completeness. Server: "${serverResult.result.substring(0, 300)}..." Database: "${dbResult.result.substring(0, 300)}..." Tests: "${testCodeResult.result.substring(0, 300)}..."`;
    const reviewResponse = await fetch(`${baseURL}/ask?role=GeneralAI&task=${encodeURIComponent(reviewPrompt)}`);
    const reviewResult = await reviewResponse.json();
    
    expect(reviewResult.role).toBe('GeneralAI');
    expect(reviewResult.result).toMatch(/review|feedback|improvement|quality/i);
    console.log('✅ GeneralAI completed code review');

    // Verify orchestration results
    console.log('🎯 Orchestration Results:');
    console.log(`- Plan: ${planResult.result.length} chars`);
    console.log(`- Server: ${serverResult.result.length} chars`);
    console.log(`- Database: ${dbResult.result.length} chars`);
    console.log(`- Test Plan: ${testPlanResult.result.length} chars`);
    console.log(`- Test Code: ${testCodeResult.result.length} chars`);
    console.log(`- Review: ${reviewResult.result.length} chars`);
    
    // Check that each AI contributed substantial content
    expect(planResult.result.length).toBeGreaterThan(500);
    expect(serverResult.result.length).toBeGreaterThan(500);
    expect(dbResult.result.length).toBeGreaterThan(300);
    expect(testCodeResult.result.length).toBeGreaterThan(500);
    expect(reviewResult.result.length).toBeGreaterThan(200);

    console.log('🎉 AI Orchestration completed successfully!');
  }, 120000); // 2 minute timeout for complex orchestration

  test('should handle iterative refinement between AIs', async () => {
    console.log('🔄 Testing iterative AI refinement...');

    // Round 1: Initial implementation
    const initialResponse = await fetch(`${baseURL}/ask?role=CodeAI&task=Write a simple user authentication function`);
    const initialResult = await initialResponse.json();
    
    // Round 2: PlannerAI suggests improvements
    const improvementPrompt = `Review this authentication code and suggest security improvements: "${initialResult.result.substring(0, 500)}..."`;
    const improvementResponse = await fetch(`${baseURL}/ask?role=PlannerAI&task=${encodeURIComponent(improvementPrompt)}`);
    const improvementResult = await improvementResponse.json();
    
    // Round 3: CodeAI implements improvements
    const refinedPrompt = `Implement these security improvements: "${improvementResult.result.substring(0, 400)}..." to the original code: "${initialResult.result.substring(0, 300)}..."`;
    const refinedResponse = await fetch(`${baseURL}/ask?role=CodeAI&task=${encodeURIComponent(refinedPrompt)}`);
    const refinedResult = await refinedResponse.json();

    expect(refinedResult.result.length).toBeGreaterThan(initialResult.result.length);
    console.log('✅ Iterative refinement completed');
  }, 60000);

  test('should demonstrate cost-effective AI routing', async () => {
    console.log('💰 Testing cost-effective AI routing...');
    
    // Get initial costs
    const statusBefore = await fetch(`${baseURL}/status`).then(r => r.json());
    const initialCost = statusBefore.costs.total;

    // Simple math task (should be cheap)
    await fetch(`${baseURL}/ask?role=GeneralAI&task=What is 10 + 5?`);
    
    // Complex planning task 
    await fetch(`${baseURL}/ask?role=PlannerAI&task=Plan a microservices architecture`);
    
    // Code generation task
    await fetch(`${baseURL}/ask?role=CodeAI&task=Write a REST API endpoint`);

    const statusAfter = await fetch(`${baseURL}/status`).then(r => r.json());
    const finalCost = statusAfter.costs.total;
    const totalCost = finalCost - initialCost;

    console.log(`💸 Total orchestration cost: $${totalCost.toFixed(4)}`);
    expect(totalCost).toBeGreaterThan(0);
    expect(totalCost).toBeLessThan(1.0); // Should be reasonable
  }, 45000);
});