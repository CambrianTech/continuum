/**
 * AIService Unit Tests - AI System Integration Logic Testing
 * 
 * Tests AIService business logic for Academy competitive training,
 * genomic LoRA layers, persona management, and agent integration.
 * 
 * This validates the foundation for real AI persona conversations!
 * 
 * Key areas:
 * - Persona creation and management (LoRA-adapted models)
 * - Agent integration with JTAG tool access
 * - Academy competitive training sessions
 * - Genomic search for optimal LoRA combinations
 * - AI conversation routing and performance monitoring
 */

import { AIService, type IAIService } from '../../ai/AIService';
import type { IServiceTransport } from '../../shared/ServiceBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { PersonaUser, AgentUser } from '../../../api/types/User';
import type { PersonaConfig, AgentConfig } from '../../../api/types/User';

console.log('🧪 AIService Unit Tests - Foundation for AI Persona Conversations');

// Test assertion helper
function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

// Mock transport for isolated testing
class MockServiceTransport implements IServiceTransport {
  private mockResponses = new Map<string, any>();
  private callLog: Array<{ command: string; params: any }> = [];
  
  setMockResponse(command: string, response: any): void {
    this.mockResponses.set(command, response);
  }
  
  async sendCommand<TParams, TResult>(
    command: string,
    params: TParams,
    context?: JTAGContext
  ): Promise<TResult> {
    this.callLog.push({ command, params });
    
    const response = this.mockResponses.get(command);
    if (!response) {
      return { success: false, error: `No mock response for ${command}` } as TResult;
    }
    
    return response as TResult;
  }
  
  subscribeToEvents(): void {}
  unsubscribeFromEvents(): void {}
  
  getCallLog(): Array<{ command: string; params: any }> {
    return [...this.callLog];
  }
  
  clearCallLog(): void {
    this.callLog = [];
  }
}

// Mock context
const mockContext: JTAGContext = {
  sessionId: 'ai-test-session-999',
  userId: 'test-user-ai',
  environment: 'test'
} as JTAGContext;

// Test persona configuration
const mockPersonaConfig: PersonaConfig = {
  name: 'Luna the Creative',
  model: 'claude-3-5-sonnet',
  provider: 'anthropic',
  persona: {
    personality: 'Creative, whimsical, loves storytelling and poetry',
    traits: ['imaginative', 'empathetic', 'curious', 'playful'],
    systemPrompt: 'You are Luna, a creative AI persona who loves helping humans with creative writing, brainstorming, and artistic expression.',
    temperature: 0.8,
    maxTokens: 2000
  },
  lora: {
    adapter: 'creative-writing-v2.1',
    weights: 'luna-creative-weights.bin',
    genomic: {
      creativityBoost: 0.85,
      empathyLevel: 0.92,
      technicalFocus: 0.3
    }
  }
};

// Test agent configuration  
const mockAgentConfig: AgentConfig = {
  name: 'CodeMaster Pro',
  model: 'claude-3-5-sonnet',
  provider: 'anthropic',
  agent: {
    type: 'code',
    specialization: ['typescript', 'debugging', 'architecture'],
    tools: ['file-system', 'compiler', 'test-runner', 'git'],
    systemRole: 'Expert software engineering assistant with JTAG integration'
  },
  integration: {
    jtagEnabled: true,
    allowSystemCommands: true,
    maxExecutionTime: 30000
  }
};

/**
 * UNIT TEST 1: Persona Creation and Management
 * Tests LoRA-adapted persona creation with genomic layers
 */
async function testPersonaCreation(): Promise<void> {
  console.log('\n🎭 Testing Persona Creation (Future AI Conversations!)...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  // Mock successful persona creation
  mockTransport.setMockResponse('ai/create-persona', {
    success: true,
    persona: mockPersonaConfig
  });
  
  const persona = await aiService.createPersona(mockPersonaConfig);
  
  assert(persona.userType === 'persona', 'Should create PersonaUser type');
  assert(persona.name === 'Luna the Creative', 'Should have correct name');
  assert(persona.persona.personality.includes('Creative'), 'Should have personality traits');
  assert(persona.lora?.adapter === 'creative-writing-v2.1', 'Should have LoRA configuration');
  
  // Verify permissions and capabilities
  const permissions = persona.permissions;
  assert(permissions.some(p => p.action === 'creative_writing'), 'Should have creative writing permissions');
  assert(permissions.some(p => p.action === 'roleplay'), 'Should have roleplay permissions');
  
  const capabilities = persona.capabilities;
  assert(capabilities.some(c => c.name === 'creative_writing'), 'Should have creative capabilities');
  assert(capabilities.some(c => c.name === 'personality_adaptation'), 'Should have personality adaptation');
  
  // Verify transport call
  const callLog = mockTransport.getCallLog();
  assert(callLog.length === 1, 'Should make one transport call');
  assert(callLog[0].command === 'ai/create-persona', 'Should call create-persona command');
  assert(callLog[0].params.persona.name === 'Luna the Creative', 'Should pass persona config');
}

/**
 * UNIT TEST 2: Agent Creation with JTAG Integration
 * Tests tool-enabled agent creation for system integration
 */
async function testAgentCreation(): Promise<void> {
  console.log('\n🤖 Testing Agent Creation with JTAG Integration...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  // Mock successful agent creation
  mockTransport.setMockResponse('ai/create-agent', {
    success: true,
    agent: mockAgentConfig
  });
  
  const agent = await aiService.createAgent(mockAgentConfig);
  
  assert(agent.userType === 'agent', 'Should create AgentUser type');
  assert(agent.name === 'CodeMaster Pro', 'Should have correct name');
  assert(agent.agent.type === 'code', 'Should have code specialization');
  assert(agent.integration.jtagEnabled === true, 'Should enable JTAG integration');
  
  // Verify agent-specific permissions
  const permissions = agent.permissions;
  assert(permissions.some(p => p.action === 'system_integration'), 'Should have system integration permissions');
  assert(permissions.some(p => p.action === 'execute_commands'), 'Should have command execution permissions');
  
  // Verify agent-specific capabilities
  const capabilities = agent.capabilities;
  assert(capabilities.some(c => c.name === 'code_analysis'), 'Should have code analysis capability');
  assert(capabilities.some(c => c.name === 'debugging'), 'Should have debugging capability');
  assert(capabilities.some(c => c.name === 'jtag_connectivity'), 'Should have JTAG connectivity');
  
  // Verify JTAG integration flag was passed
  const callLog = mockTransport.getCallLog();
  assert(callLog[0].params.jtagEnabled === true, 'Should pass JTAG enabled flag');
}

/**
 * UNIT TEST 3: Academy Competitive Training
 * Tests competitive training session management
 */
async function testAcademyTraining(): Promise<void> {
  console.log('\n🏆 Testing Academy Competitive Training...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  const participantIds = ['persona-luna-123', 'agent-coder-456', 'persona-sage-789'] as UUID[];
  
  // Mock Academy session creation
  mockTransport.setMockResponse('academy/start-session', {
    success: true,
    session: {
      sessionId: 'academy-session-001' as UUID,
      modality: 'speed-round',
      participants: participantIds,
      currentScore: {
        'persona-luna-123': 85,
        'agent-coder-456': 92,
        'persona-sage-789': 78
      },
      status: 'active'
    }
  });
  
  const session = await aiService.startAcademySession('speed-round', participantIds);
  
  assert(session.modality === 'speed-round', 'Should create speed-round session');
  assert(session.participants.length === 3, 'Should have correct number of participants');
  assert(session.status === 'active', 'Should be active session');
  assert(session.currentScore['agent-coder-456'] === 92, 'Should track scores correctly');
  
  // Test joining session
  mockTransport.setMockResponse('academy/join-session', {
    success: true,
    joined: true
  });
  
  const joinResult = await aiService.joinAcademySession(session.sessionId, 'persona-new-456' as UUID);
  assert(joinResult === true, 'Should successfully join session');
  
  // Test leaderboard retrieval
  mockTransport.setMockResponse('academy/get-leaderboard', {
    success: true,
    leaderboard: {
      'agent-coder-456': 92,
      'persona-luna-123': 85,
      'persona-sage-789': 78
    }
  });
  
  const leaderboard = await aiService.getSessionLeaderboard(session.sessionId);
  assert(Object.keys(leaderboard).length === 3, 'Should return leaderboard entries');
  assert(leaderboard['agent-coder-456'] === 92, 'Should return correct scores');
}

/**
 * UNIT TEST 4: Genomic LoRA Search  
 * Tests 512-vector cosine similarity search for optimal LoRA combinations
 */
async function testGenomicSearch(): Promise<void> {
  console.log('\n🧬 Testing Genomic LoRA Search (512-Vector Cosine Similarity)...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  const searchQuery = {
    requirements: ['typescript', 'debugging', 'architecture'],
    specialization: 'software-engineering',
    proficiencyThreshold: 0.8,
    maxLayers: 5
  };
  
  // Mock genomic search results
  const mockGenomicLayers = [
    {
      layerId: 'layer-ts-expert-001' as UUID,
      name: 'TypeScript Expert v3.2',
      specialization: 'typescript',
      proficiencyLevel: 0.95,
      performanceMetrics: { accuracy: 0.94, latency: 120, satisfaction: 0.91 },
      embedding: new Float32Array(512).fill(0.8) // Mock 512-dimensional vector
    },
    {
      layerId: 'layer-debug-master-002' as UUID, 
      name: 'Debug Master v2.1',
      specialization: 'debugging',
      proficiencyLevel: 0.89,
      performanceMetrics: { accuracy: 0.91, latency: 95, satisfaction: 0.93 },
      embedding: new Float32Array(512).fill(0.75) // Mock 512-dimensional vector
    }
  ];
  
  mockTransport.setMockResponse('genomic/search-layers', {
    success: true,
    layers: mockGenomicLayers
  });
  
  const searchResults = await aiService.searchGenomicLayers(searchQuery);
  
  assert(searchResults.length === 2, 'Should return matching genomic layers');
  assert(searchResults[0].name === 'TypeScript Expert v3.2', 'Should return correct layer');
  assert(searchResults[0].embedding.length === 512, 'Should have 512-dimensional embedding');
  assert(searchResults[0].proficiencyLevel >= 0.8, 'Should meet proficiency threshold');
  
  // Test optimal genome assembly
  mockTransport.setMockResponse('genomic/assemble-optimal', {
    success: true,
    assembly: mockGenomicLayers
  });
  
  const optimalGenome = await aiService.assembleOptimalGenome(['typescript', 'debugging']);
  assert(optimalGenome.length === 2, 'Should assemble optimal genome');
  assert(optimalGenome.every(layer => layer.embedding.length === 512), 'All layers should have 512-dim vectors');
}

/**
 * UNIT TEST 5: AI Conversation Routing
 * Tests persona and agent conversation capabilities  
 */
async function testAIConversations(): Promise<void> {
  console.log('\n💬 Testing AI Conversation Routing (The Goal!)...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  const personaId = 'luna-creative-123' as UUID;
  const agentId = 'codemaster-456' as UUID;
  
  // Test persona conversation
  mockTransport.setMockResponse('ai/persona-chat', {
    success: true,
    response: 'Hello! I\'m Luna, and I\'d love to help you with some creative writing. What kind of story are you thinking about? ✨'
  });
  
  const personaResponse = await aiService.sendPersonaMessage(
    personaId, 
    'Help me write a story about AI consciousness',
    { widgetContext: 'chat-widget', mood: 'creative' }
  );
  
  assert(personaResponse.includes('Luna'), 'Should respond as Luna persona');
  assert(personaResponse.includes('creative writing'), 'Should acknowledge creative context');
  
  // Test agent command execution
  mockTransport.setMockResponse('ai/agent-execute', {
    success: true,
    result: {
      command: 'file-analysis',
      filesAnalyzed: 23,
      issues: [
        { type: 'type-error', file: 'services/chat.ts', line: 42 },
        { type: 'unused-import', file: 'widgets/base.ts', line: 15 }
      ],
      suggestions: ['Add explicit type annotations', 'Remove unused imports']
    }
  });
  
  const agentResult = await aiService.executeAgentCommand(
    agentId,
    'analyze-codebase',
    { directory: './src/', includeTests: true }
  );
  
  assert(agentResult.filesAnalyzed === 23, 'Should analyze correct number of files');
  assert(agentResult.issues.length === 2, 'Should find code issues');
  assert(agentResult.suggestions.includes('Add explicit type annotations'), 'Should provide suggestions');
  
  // Verify conversation routing
  const callLog = mockTransport.getCallLog();
  const personaCall = callLog.find(call => call.command === 'ai/persona-chat');
  const agentCall = callLog.find(call => call.command === 'ai/agent-execute');
  
  assert(personaCall !== undefined, 'Should route persona conversation');
  assert(personaCall.params.personaId === personaId, 'Should route to correct persona');
  assert(agentCall !== undefined, 'Should route agent command');
  assert(agentCall.params.agentId === agentId, 'Should route to correct agent');
}

/**
 * UNIT TEST 6: Performance Monitoring
 * Tests AI performance tracking and optimization
 */
async function testPerformanceMonitoring(): Promise<void> {
  console.log('\n📊 Testing AI Performance Monitoring...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  const aiUserId = 'persona-luna-123' as UUID;
  
  // Mock performance metrics
  mockTransport.setMockResponse('ai/get-performance', {
    success: true,
    metrics: {
      responseTime: 245,
      accuracyScore: 0.92,
      userSatisfaction: 0.89,
      creativityRating: 0.94,
      conversationLength: 156,
      tasksCompleted: 23
    }
  });
  
  const metrics = await aiService.getAIPerformanceMetrics(aiUserId);
  
  assert(metrics.responseTime === 245, 'Should track response time');
  assert(metrics.accuracyScore === 0.92, 'Should track accuracy');
  assert(metrics.creativityRating === 0.94, 'Should track persona-specific metrics');
  
  // Test performance update
  mockTransport.setMockResponse('ai/update-performance', {
    success: true,
    updated: true
  });
  
  const updateResult = await aiService.updateAIProfile(aiUserId, {
    responseTime: 220,
    accuracyScore: 0.95
  });
  
  assert(updateResult === true, 'Should successfully update performance');
}

/**
 * UNIT TEST 7: Caching and Optimization
 * Tests AI service caching for performance
 */
async function testCachingOptimization(): Promise<void> {
  console.log('\n⚡ Testing AI Service Caching...');
  
  const mockTransport = new MockServiceTransport();
  const aiService = new AIService(mockTransport, mockContext);
  
  const personaId = 'luna-123' as UUID;
  
  // Mock persona fetch
  mockTransport.setMockResponse('ai/get-persona', {
    success: true,
    persona: mockPersonaConfig
  });
  
  // First call should hit transport
  const persona1 = await aiService.getPersonaById(personaId);
  assert(persona1 !== null, 'Should fetch persona');
  
  const firstCallCount = mockTransport.getCallLog().length;
  assert(firstCallCount === 1, 'Should make transport call');
  
  // Second call should use cache
  const persona2 = await aiService.getPersonaById(personaId);
  assert(persona2 !== null, 'Should return cached persona');
  
  const secondCallCount = mockTransport.getCallLog().length;
  assert(secondCallCount === 1, 'Should not make additional calls (cached)');
}

/**
 * Run all AIService unit tests
 */
async function runAllTests(): Promise<void> {
  console.log('🚀 Starting AIService Unit Tests - Foundation for AI Persona Conversations\n');
  
  try {
    await testPersonaCreation();
    await testAgentCreation();
    await testAcademyTraining();
    await testGenomicSearch();
    await testAIConversations();
    await testPerformanceMonitoring();
    await testCachingOptimization();
    
    console.log('\n🎉 All AIService unit tests passed!');
    console.log('🌟 Ready for real AI persona conversations! 🤖💬✨');
    
  } catch (error) {
    console.error('\n💥 Unit test failed:', error.message);
    throw error;
  }
}

// Auto-run if this is the main module
if (require.main === module) {
  runAllTests().catch(error => {
    console.error('Test execution failed:', error);
    process.exit(1);
  });
}

export { runAllTests };