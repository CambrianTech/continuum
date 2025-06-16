/**
 * Modular UI Generator Test
 * Tests the new component-based UI structure
 */

describe('UI Generator', () => {
  let UIGenerator;
  let mockContinuum;

  beforeEach(() => {
    // Setup mock continuum
    mockContinuum = {
      academyInterface: {
        generateAcademyHTML: () => '<div>Academy HTML</div>',
        generateAcademyJS: () => 'console.log("Academy JS");'
      }
    };

    // Import the UI generator
    UIGenerator = require('../../../../src/ui/UIGenerator.cjs');
  });

  test('should create UIGenerator instance', () => {
    const generator = new UIGenerator(mockContinuum);
    expect(generator).toBeInstanceOf(UIGenerator);
    expect(generator.continuum).toBe(mockContinuum);
  });

  test('should generate HTML with web components', () => {
    const generator = new UIGenerator(mockContinuum);
    const html = generator.generateHTML();
    
    expect(html).toContain('<!DOCTYPE html>');
    expect(html).toContain('<user-selector>');
    expect(html).toContain('chat-header');
    expect(html).toContain('chat-area');
    expect(html).toContain('<room-tabs>');
    expect(html).toContain('<status-pill');
    expect(html).toContain('<academy-section>');
  });

  test('should include component script imports', () => {
    const generator = new UIGenerator(mockContinuum);
    const html = generator.generateHTML();
    
    expect(html).toContain('<script src="/components/UserSelector.js">');
    expect(html).toContain('<script src="/components/ChatHeader.js">');
    expect(html).toContain('<script src="/components/ChatArea.js">');
    expect(html).toContain('<script src="/components/RoomTabs.js">');
    expect(html).toContain('<script src="/components/StatusPill.js">');
    expect(html).toContain('<script src="/components/AcademySection.js">');
  });

  test('should include application logic', () => {
    const generator = new UIGenerator(mockContinuum);
    const html = generator.generateHTML();
    
    expect(html).toContain('initializeComponents');
    expect(html).toContain('connectWebSocket');
    expect(html).toContain('setupEventListeners');
    expect(html).toContain('handleWebSocketMessage');
  });

  test('should maintain existing styles', () => {
    const generator = new UIGenerator(mockContinuum);
    const html = generator.generateHTML();
    
    expect(html).toContain('.app-container');
    expect(html).toContain('.sidebar');
    expect(html).toContain('.main-content');
    expect(html).toContain('linear-gradient');
  });

  test('should have reduced HTML size compared to old generator', () => {
    const generator = new UIGenerator(mockContinuum);
    const html = generator.generateHTML();
    
    // Modular version should be significantly smaller than the old monolithic version
    expect(html.length).toBeLessThan(50000); // Old version was 55k+ characters
    expect(html.length).toBeGreaterThan(5000);  // Still substantial
  });
});

describe('Component File Structure', () => {
  const fs = require('fs');
  const path = require('path');
  
  const componentsDir = path.join(__dirname, '../../src/ui/components');
  
  test('should have all required component files', () => {
    const expectedFiles = [
      'AgentSelector.js',
      'ChatHeader.js', 
      'ChatArea.js',
      'RoomTabs.js',
      'StatusPill.js',
      'AcademySection.js'
    ];

    expectedFiles.forEach(file => {
      const filePath = path.join(componentsDir, file);
      expect(fs.existsSync(filePath)).toBe(true);
    });
  });

  test('should have component files with proper structure', () => {
    const componentFiles = [
      'AgentSelector.js',
      'ChatHeader.js',
      'ChatArea.js', 
      'RoomTabs.js',
      'StatusPill.js',
      'AcademySection.js'
    ];

    componentFiles.forEach(file => {
      const filePath = path.join(componentsDir, file);
      const content = fs.readFileSync(filePath, 'utf8');
      
      // Should be proper web components
      expect(content).toMatch(/class \w+ extends HTMLElement/);
      expect(content).toMatch(/constructor\(\)/);
      expect(content).toMatch(/connectedCallback\(\)/);
      expect(content).toMatch(/attachShadow/);
      expect(content).toMatch(/customElements\.define/);
      
      // Should have module exports for Node.js
      expect(content).toMatch(/module\.exports/);
    });
  });
});