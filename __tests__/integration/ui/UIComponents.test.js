/**
 * UI Components Integration Test
 * Tests that the new component system works with the existing UI
 */

const { JSDOM } = require('jsdom');

describe('UI Components Integration', () => {
  let dom, document, window;

  beforeEach(() => {
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
  });

  afterEach(() => {
    delete global.window;
    delete global.document;
    delete global.HTMLElement;
    delete global.CustomEvent;
    delete global.customElements;
  });

  test('UIGenerator should include component scripts', () => {
    const UIGenerator = require('../../src/ui/UIGenerator.cjs');
    const ui = new UIGenerator({});
    const html = ui.generateHTML();

    expect(html).toContain('ComponentLoader.js');
    expect(html).toContain('UserSelector.js');
    expect(html).toContain('component-system-ready');
  });

  test('ComponentLoader should load properly', () => {
    const ComponentLoader = require('../../src/ui/utils/ComponentLoader.js');
    const loader = new ComponentLoader();

    expect(loader).toBeDefined();
    expect(loader.loadedComponents).toBeDefined();
    expect(loader.componentInstances).toBeDefined();
  });

  test('AgentSelector component should be loadable', () => {
    const AgentSelector = require('../../src/ui/components/AgentSelector.js');
    
    expect(AgentSelector).toBeDefined();
    expect(typeof AgentSelector).toBe('function');
    
    const selector = new AgentSelector();
    expect(selector.tagName).toBe('AGENT-SELECTOR');
  });

  test('Component system should not break existing functionality', () => {
    // Test that we can still create UIGenerator
    const UIGenerator = require('../../src/ui/UIGenerator.cjs');
    const ui = new UIGenerator({});
    
    // Test that HTML generation still works
    const html = ui.generateHTML();
    expect(html).toContain('class="sidebar"');
    expect(html).toContain('class="agent-selector"');
    expect(html).toContain('Available Agents');
    
    // Verify we haven't broken the existing structure
    expect(html).toContain('PlannerAI');
    expect(html).toContain('CodeAI');
    expect(html).toContain('GeneralAI');
  });

  test('AgentSelector component should work with remote agents', () => {
    const AgentSelector = require('../../src/ui/components/AgentSelector.js');
    const selector = new AgentSelector();
    
    const remoteAgents = [
      {
        id: 'claude-remote',
        name: 'Claude Sonnet',
        type: 'ai',
        status: 'connected',
        source: 'remote',
        avatar: '🤖',
        role: 'Assistant',
        gradient: 'linear-gradient(135deg, #00d4ff, #0099cc)',
        hostInfo: { hostname: 'claude.ai' },
        messageCount: 0
      },
      {
        id: 'human-dev',
        name: 'Developer',
        type: 'human',
        status: 'connected',
        source: 'remote',
        avatar: '👤',
        role: 'Software Engineer',
        gradient: 'linear-gradient(135deg, #ff6b6b, #ee5a5a)',
        hostInfo: { hostname: 'dev-machine' },
        messageCount: 5
      }
    ];
    
    selector.updateRemoteAgents(remoteAgents);
    
    expect(selector.remoteAgents).toHaveLength(2);
    expect(selector.remoteAgents[0].name).toBe('Claude Sonnet');
    expect(selector.remoteAgents[1].name).toBe('Developer');
  });

  test('Components should maintain backward compatibility', () => {
    // Ensure the original agent selector HTML structure still exists
    const UIGenerator = require('../../src/ui/UIGenerator.cjs');
    const ui = new UIGenerator({});
    const html = ui.generateHTML();
    
    // Check for key elements that existing code depends on
    expect(html).toContain('id="agent-auto"');
    expect(html).toContain('onclick="selectAgent(\'auto\')"');
    expect(html).toContain('agent-dropdown-btn');
    expect(html).toContain('toggleAgentInfo');
    
    // Check that the agent details panel is still there
    expect(html).toContain('id="agent-details-panel"');
    expect(html).toContain('agent-details-panel');
  });
});