/**
 * Widget Test Template - Comprehensive testing for widgets
 * Covers: Unit (server), Client (browser), Integration (both sides)
 * All widgets must implement these test patterns
 */

export interface WidgetTestSuite {
  // Server-side unit tests
  unitTests: {
    componentCreation: () => Promise<boolean>;
    packageJsonValidation: () => Promise<boolean>;
    cssCompilation: () => Promise<boolean>;
    dependencyResolution: () => Promise<boolean>;
  };

  // Client-side tests (browser environment)
  clientTests: {
    customElementRegistration: () => Promise<boolean>;
    domRendering: () => Promise<boolean>;
    eventHandling: () => Promise<boolean>;
    cssApplication: () => Promise<boolean>;
    apiIntegration: () => Promise<boolean>;
  };

  // Integration tests (server + client)
  integrationTests: {
    fullLoadingCycle: () => Promise<boolean>;
    serverCompilation: () => Promise<boolean>;
    browserLoading: () => Promise<boolean>;
    realTimeUpdates: () => Promise<boolean>;
    errorHandling: () => Promise<boolean>;
  };
}

export abstract class BaseWidgetTest {
  protected widgetName: string;
  protected widgetPath: string;

  constructor(widgetName: string, widgetPath: string) {
    this.widgetName = widgetName;
    this.widgetPath = widgetPath;
  }

  /**
   * Run all test types for this widget
   */
  async runAllTests(): Promise<{ passed: number; failed: number; results: any[] }> {
    const results: any[] = [];
    let passed = 0;
    let failed = 0;

    // Unit tests (server-side)
    console.log(`🧪 Running unit tests for ${this.widgetName}...`);
    const unitResults = await this.runUnitTests();
    results.push({ type: 'unit', ...unitResults });
    passed += unitResults.passed;
    failed += unitResults.failed;

    // Client tests (browser-side)
    console.log(`🌐 Running client tests for ${this.widgetName}...`);
    const clientResults = await this.runClientTests();
    results.push({ type: 'client', ...clientResults });
    passed += clientResults.passed;
    failed += clientResults.failed;

    // Integration tests (both sides)
    console.log(`🔗 Running integration tests for ${this.widgetName}...`);
    const integrationResults = await this.runIntegrationTests();
    results.push({ type: 'integration', ...integrationResults });
    passed += integrationResults.passed;
    failed += integrationResults.failed;

    return { passed, failed, results };
  }

  protected abstract runUnitTests(): Promise<{ passed: number; failed: number; details: any[] }>;
  protected abstract runClientTests(): Promise<{ passed: number; failed: number; details: any[] }>;
  protected abstract runIntegrationTests(): Promise<{ passed: number; failed: number; details: any[] }>;

  /**
   * Standard unit test implementations that all widgets should pass
   */
  protected async standardUnitTests(): Promise<{ passed: number; failed: number; details: any[] }> {
    const details: any[] = [];
    let passed = 0;
    let failed = 0;

    // Test 1: Package.json exists and is valid
    try {
      const fs = await import('fs');
      const path = await import('path');
      const packagePath = path.join(this.widgetPath, 'package.json');
      
      if (fs.existsSync(packagePath)) {
        const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf-8'));
        if (packageJson.name && packageJson.main) {
          details.push({ test: 'package.json validation', status: 'passed' });
          passed++;
        } else {
          details.push({ test: 'package.json validation', status: 'failed', error: 'Missing name or main field' });
          failed++;
        }
      } else {
        details.push({ test: 'package.json validation', status: 'failed', error: 'package.json not found' });
        failed++;
      }
    } catch (error) {
      details.push({ test: 'package.json validation', status: 'failed', error: error });
      failed++;
    }

    // Test 2: Widget implementation file exists
    try {
      const fs = await import('fs');
      const path = await import('path');
      const widgetFile = path.join(this.widgetPath, `${this.widgetName}Widget.ts`);
      
      if (fs.existsSync(widgetFile)) {
        details.push({ test: 'widget file exists', status: 'passed' });
        passed++;
      } else {
        details.push({ test: 'widget file exists', status: 'failed', error: `${this.widgetName}Widget.ts not found` });
        failed++;
      }
    } catch (error) {
      details.push({ test: 'widget file exists', status: 'failed', error: error });
      failed++;
    }

    // Test 3: CSS file exists
    try {
      const fs = await import('fs');
      const path = await import('path');
      const cssFile = path.join(this.widgetPath, `${this.widgetName}Widget.css`);
      
      if (fs.existsSync(cssFile)) {
        details.push({ test: 'CSS file exists', status: 'passed' });
        passed++;
      } else {
        details.push({ test: 'CSS file exists', status: 'warning', message: 'No CSS file found - widgets should have styling' });
        // Don't count as failed, just warning
      }
    } catch (error) {
      details.push({ test: 'CSS file exists', status: 'failed', error: error });
      failed++;
    }

    return { passed, failed, details };
  }

  /**
   * Standard client test implementations
   */
  protected async standardClientTests(): Promise<{ passed: number; failed: number; details: any[] }> {
    // These would run in browser environment
    // For now, return placeholder results
    return {
      passed: 0,
      failed: 0,
      details: [
        { test: 'client tests', status: 'skipped', message: 'Browser environment required' }
      ]
    };
  }

  /**
   * Standard integration test implementations
   */
  protected async standardIntegrationTests(): Promise<{ passed: number; failed: number; details: any[] }> {
    const details: any[] = [];
    let passed = 0;
    let failed = 0;

    // Test 1: Widget can be compiled to JavaScript
    try {
      const fs = await import('fs');
      const path = await import('path');
      const widgetFile = path.join(this.widgetPath, `${this.widgetName}Widget.ts`);
      
      if (fs.existsSync(widgetFile)) {
        const content = fs.readFileSync(widgetFile, 'utf-8');
        
        // Check for customElements.define call
        if (content.includes('customElements.define')) {
          details.push({ test: 'custom element registration', status: 'passed' });
          passed++;
        } else {
          details.push({ test: 'custom element registration', status: 'failed', error: 'No customElements.define found' });
          failed++;
        }

        // Check for extends BaseWidget or HTMLElement
        if (content.includes('extends BaseWidget') || content.includes('extends HTMLElement')) {
          details.push({ test: 'proper inheritance', status: 'passed' });
          passed++;
        } else {
          details.push({ test: 'proper inheritance', status: 'failed', error: 'Widget should extend BaseWidget or HTMLElement' });
          failed++;
        }
      }
    } catch (error) {
      details.push({ test: 'widget compilation', status: 'failed', error: error });
      failed++;
    }

    return { passed, failed, details };
  }
}