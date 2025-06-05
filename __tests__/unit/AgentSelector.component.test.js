/**
 * Agent Selector Component Tests
 * Tests for the AgentSelector web component
 */

const { JSDOM } = require('jsdom');

describe('AgentSelector Component', () => {
  let dom, document, window, AgentSelector;

  beforeEach(() => {
    // Setup JSDOM environment
    dom = new JSDOM(`
      <!DOCTYPE html>
      <html>
        <body>
          <div id="test-container"></div>
        </body>
      </html>
    `, {
      url: "http://localhost",
      pretendToBeVisual: true,
      resources: "usable"
    });

    window = dom.window;
    document = window.document;
    global.window = window;
    global.document = document;
    global.HTMLElement = window.HTMLElement;
    global.CustomEvent = window.CustomEvent;
    global.customElements = window.customElements;

    // Load the component - get the class directly from the module
    AgentSelector = require('../../src/ui/components/AgentSelector.js');
  });

  afterEach(() => {
    // Cleanup
    delete global.window;
    delete global.document;
    delete global.HTMLElement;
    delete global.CustomEvent;
    delete global.customElements;
  });

  test('should create AgentSelector component', () => {
    const selector = new AgentSelector();
    expect(selector).toBeDefined();
    expect(selector.tagName).toBe('AGENT-SELECTOR');
  });

  test('should have default agents', () => {
    const selector = new AgentSelector();
    const agents = selector.getDefaultAgents();
    
    expect(agents).toHaveLength(5);
    expect(agents[0].id).toBe('auto');
    expect(agents[0].name).toBe('Auto Route');
    expect(agents.find(a => a.id === 'PlannerAI')).toBeDefined();
    expect(agents.find(a => a.id === 'CodeAI')).toBeDefined();
  });

  test('should set initial selected agent to auto', () => {
    const selector = new AgentSelector();
    expect(selector.selectedAgent).toBe('auto');
  });

  test('should select agent and update state', () => {
    const selector = new AgentSelector();
    document.body.appendChild(selector);
    
    selector.selectAgent('PlannerAI');
    expect(selector.selectedAgent).toBe('PlannerAI');
  });

  test('should emit agent-selected event when agent is selected', (done) => {
    const selector = new AgentSelector();
    document.body.appendChild(selector);
    
    selector.addEventListener('agent-selected', (event) => {
      expect(event.detail.agentId).toBe('CodeAI');
      done();
    });
    
    selector.selectAgent('CodeAI');
  });

  test('should call onAgentSelect callback when agent is selected', () => {
    const selector = new AgentSelector();
    const callback = jest.fn();
    
    selector.setOnAgentSelect(callback);
    selector.selectAgent('GeneralAI');
    
    expect(callback).toHaveBeenCalledWith('GeneralAI');
  });

  test('should update remote agents and re-render', () => {
    const selector = new AgentSelector();
    document.body.appendChild(selector);
    
    const remoteAgents = [
      {
        id: 'remote-1',
        name: 'Remote Claude',
        type: 'ai',
        status: 'connected',
        source: 'remote',
        avatar: '🤖',
        role: 'Assistant',
        gradient: 'linear-gradient(135deg, #00d4ff, #0099cc)',
        hostInfo: { hostname: 'remote-server' },
        messageCount: 5
      }
    ];
    
    selector.updateRemoteAgents(remoteAgents);
    expect(selector.remoteAgents).toEqual(remoteAgents);
  });

  test('should distinguish between human and AI remote agents', () => {
    const selector = new AgentSelector();
    
    const humanAgent = {
      id: 'human-1',
      name: 'John Doe',
      type: 'human',
      status: 'connected',
      source: 'remote',
      avatar: '👤',
      role: 'Developer',
      gradient: 'linear-gradient(135deg, #ff6b6b, #ee5a5a)'
    };
    
    const aiAgent = {
      id: 'ai-1',
      name: 'Claude',
      type: 'ai',
      status: 'connected',
      source: 'remote',
      avatar: '🤖',
      role: 'Assistant',
      gradient: 'linear-gradient(135deg, #00d4ff, #0099cc)'
    };
    
    selector.updateRemoteAgents([humanAgent, aiAgent]);
    
    // Check that we can filter by type
    const remoteHumans = selector.remoteAgents.filter(agent => 
      agent.type === 'human' || agent.type === 'user');
    const remoteAIs = selector.remoteAgents.filter(agent => 
      agent.type === 'ai' || agent.type === 'system');
    
    expect(remoteHumans).toHaveLength(1);
    expect(remoteAIs).toHaveLength(1);
    expect(remoteHumans[0].name).toBe('John Doe');
    expect(remoteAIs[0].name).toBe('Claude');
  });

  test('should generate correct HTML for agent items', () => {
    const selector = new AgentSelector();
    
    const agent = {
      id: 'test-agent',
      name: 'Test Agent',
      role: 'Testing',
      avatar: '🧪',
      gradient: 'linear-gradient(135deg, #FF0000, #00FF00)',
      status: 'online',
      type: 'ai'
    };
    
    const html = selector.generateAgentHTML(agent);
    
    expect(html).toContain('test-agent');
    expect(html).toContain('Test Agent');
    expect(html).toContain('Testing');
    expect(html).toContain('🧪');
    expect(html).toContain('agent-dropdown-btn');
  });

  test('should not show dropdown button for auto agent', () => {
    const selector = new AgentSelector();
    
    const autoAgent = selector.getDefaultAgents().find(a => a.id === 'auto');
    const html = selector.generateAgentHTML(autoAgent);
    
    expect(html).not.toContain('agent-dropdown-btn');
  });

  test('should emit agent-info-requested event when dropdown button is clicked', (done) => {
    const selector = new AgentSelector();
    document.body.appendChild(selector);
    
    selector.addEventListener('agent-info-requested', (event) => {
      expect(event.detail.agentId).toBe('PlannerAI');
      done();
    });
    
    selector.showAgentInfo('PlannerAI');
  });

  test('should handle setOnAgentInfo callback', () => {
    const selector = new AgentSelector();
    const callback = jest.fn();
    
    selector.setOnAgentInfo(callback);
    selector.showAgentInfo('CodeAI');
    
    expect(callback).toHaveBeenCalledWith('CodeAI');
  });

  test('should generate remote sections correctly', () => {
    const selector = new AgentSelector();
    
    const agents = [
      { id: '1', name: 'Test', type: 'human', source: 'remote' }
    ];
    
    const html = selector.generateRemoteSection('Test Section', agents, '#ff0000');
    
    expect(html).toContain('Test Section');
    expect(html).toContain('remote-section');
    expect(html).toContain('color: #ff0000');
  });

  test('should return empty string for empty remote sections', () => {
    const selector = new AgentSelector();
    
    const html = selector.generateRemoteSection('Empty', [], '#ff0000');
    
    expect(html).toBe('');
  });
});