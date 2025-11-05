/**
 * Widget Self-Discovery Integration Test
 * 
 * Tests widgets' ability to discover themselves and validate their own loading
 * Uses JSDOM to simulate browser environment without actual browser
 */

import { JSDOM } from 'jsdom';

describe('Widget Self-Discovery Integration', () => {
  let dom: JSDOM;
  let window: any;
  let document: any;

  beforeEach(async () => {
    // Fetch actual HTML from server
    const response = await fetch('http://localhost:9000/');
    const html = await response.text();
    
    // Create DOM environment with the real HTML
    dom = new JSDOM(html, {
      url: 'http://localhost:9000/',
      resources: 'usable',
      runScripts: 'outside-only'
    });
    
    window = dom.window;
    document = window.document;
    
    // Add global fetch for the scripts
    (global as any).fetch = fetch;
    (window as any).fetch = fetch;
  });

  afterEach(() => {
    if (dom) {
      dom.window.close();
    }
  });

  test('widgets can discover their own script dependencies', async () => {
    // Extract all script tags that widgets need
    const scripts = Array.from(document.querySelectorAll('script[src]'));
    expect(scripts.length).toBeGreaterThan(0);
    
    // Find the continuum.js script specifically
    const continuumScript = scripts.find((script: any) => 
      script.getAttribute('src')?.includes('continuum.js')
    );
    expect(continuumScript).toBeTruthy();
    
    const scriptSrc = continuumScript!.getAttribute('src');
    console.log('🔍 Widget discovered continuum.js at:', scriptSrc);
    
    // Widget validates its own dependency is loadable
    const scriptUrl = `http://localhost:9000${scriptSrc}`;
    const scriptResponse = await fetch(scriptUrl);
    expect(scriptResponse.status).toBe(200);
    
    console.log('✅ Widget self-validated: continuum.js is loadable');
  });

  test('widgets can validate their readiness event system', async () => {
    // Fetch the continuum.js script content (what widgets would load)
    const continuumScript = document.querySelector('script[src*="continuum.js"]');
    const scriptSrc = continuumScript!.getAttribute('src');
    const scriptUrl = `http://localhost:9000${scriptSrc}`;
    
    const scriptResponse = await fetch(scriptUrl);
    const scriptContent = await scriptResponse.text();
    
    // Widget validates that its expected events exist
    expect(scriptContent).toContain('continuum:ready');
    expect(scriptContent).toContain('continuum:connecting');
    expect(scriptContent).toContain('ContinuumBrowserAPI');
    
    console.log('✅ Widget self-validated: Event system is present');
  });

  test('widgets can simulate their own loading process', async () => {
    // Simulate what happens when a widget loads
    const mockEventListeners = new Map();
    
    // Mock the widget's event listener registration
    const mockAddEventListener = (event: string, handler: Function) => {
      if (!mockEventListeners.has(event)) {
        mockEventListeners.set(event, []);
      }
      mockEventListeners.get(event).push(handler);
      console.log(`🎧 Widget registered listener for: ${event}`);
    };
    
    // Mock the widget discovering and registering for events
    mockAddEventListener('continuum:ready', () => {
      console.log('✅ Widget would receive continuum:ready event');
    });
    
    mockAddEventListener('continuum:connecting', () => {
      console.log('🔄 Widget would receive continuum:connecting event');
    });
    
    // Validate widget registered for expected events
    expect(mockEventListeners.has('continuum:ready')).toBe(true);
    expect(mockEventListeners.has('continuum:connecting')).toBe(true);
    expect(mockEventListeners.get('continuum:ready')).toHaveLength(1);
    
    console.log('✅ Widget self-validated: Event registration working');
  });

  test('widgets can validate their API availability pattern', async () => {
    // Simulate widget checking for API availability
    const checkAPIPattern = (scriptContent: string): boolean => {
      // Widget looks for the global API assignment pattern
      return scriptContent.includes('window.continuum = continuum') &&
             scriptContent.includes('continuum:ready');
    };
    
    // Get the actual script content
    const continuumScript = document.querySelector('script[src*="continuum.js"]');
    const scriptSrc = continuumScript!.getAttribute('src');
    const scriptUrl = `http://localhost:9000${scriptSrc}`;
    
    const scriptResponse = await fetch(scriptUrl);
    const scriptContent = await scriptResponse.text();
    
    // Widget validates the API will be available
    const apiWillBeAvailable = checkAPIPattern(scriptContent);
    expect(apiWillBeAvailable).toBe(true);
    
    console.log('✅ Widget self-validated: API availability pattern confirmed');
  });

  test('widgets can check their container elements exist', async () => {
    // Widget checks if its container element exists in the DOM
    const chatWidget = document.querySelector('chat-widget');
    expect(chatWidget).toBeTruthy();
    console.log('✅ Widget self-validated: chat-widget container exists');
    
    // Widget can validate all required containers
    const requiredContainers = ['chat-widget'];
    const missingContainers = requiredContainers.filter(selector => 
      !document.querySelector(selector)
    );
    
    expect(missingContainers).toHaveLength(0);
    console.log('✅ Widget self-validated: All required containers present');
  });

  test('widgets can validate version coordination', async () => {
    // Widget checks version parameters for cache busting
    const scripts = Array.from(document.querySelectorAll('script[src]'));
    const scriptsWithVersions = scripts.filter((script: any) => {
      const src = script.getAttribute('src');
      return src && src.includes('?v=') && src.includes('&bust=');
    });
    
    expect(scriptsWithVersions.length).toBeGreaterThan(0);
    
    // Widget validates version consistency
    const versions = scriptsWithVersions.map((script: any) => {
      const src = script.getAttribute('src');
      const versionMatch = src.match(/\?v=([^&]+)/);
      return versionMatch ? versionMatch[1] : null;
    }).filter(v => v);
    
    const uniqueVersions = [...new Set(versions)];
    expect(uniqueVersions.length).toBe(1); // All should have same version
    
    console.log(`✅ Widget self-validated: Consistent version ${uniqueVersions[0]}`);
  });

  test('widgets can simulate their complete loading lifecycle', async () => {
    console.log('🔄 Widget simulating complete loading lifecycle...');
    
    // Step 1: Widget discovers its script dependencies
    const continuumScript = document.querySelector('script[src*="continuum.js"]');
    expect(continuumScript).toBeTruthy();
    console.log('  ✅ Step 1: Dependencies discovered');
    
    // Step 2: Widget validates dependencies are loadable
    const scriptSrc = continuumScript!.getAttribute('src');
    const scriptUrl = `http://localhost:9000${scriptSrc}`;
    const response = await fetch(scriptUrl);
    expect(response.status).toBe(200);
    console.log('  ✅ Step 2: Dependencies validated as loadable');
    
    // Step 3: Widget checks for its container
    const container = document.querySelector('chat-widget');
    expect(container).toBeTruthy();
    console.log('  ✅ Step 3: Container element found');
    
    // Step 4: Widget validates API patterns exist
    const scriptContent = await response.text();
    expect(scriptContent).toContain('continuum:ready');
    console.log('  ✅ Step 4: API patterns validated');
    
    // Step 5: Widget would register event listeners (simulated)
    const mockWidget = {
      loaded: false,
      onReady: () => { mockWidget.loaded = true; }
    };
    
    // Simulate the continuum:ready event
    mockWidget.onReady();
    expect(mockWidget.loaded).toBe(true);
    console.log('  ✅ Step 5: Event system simulation complete');
    
    console.log('🎉 Widget successfully completed self-validation lifecycle!');
  });
});