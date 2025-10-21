/**
 * Widget Inspect Command Unit Tests
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { WidgetInspectCommand } from '../../WidgetInspectCommand';

describe('WidgetInspectCommand', () => {
  test('should have correct command definition', () => {
    const definition = WidgetInspectCommand.getDefinition();
    
    assert.strictEqual(definition.name, 'widget-inspect');
    assert.strictEqual(definition.category, 'browser');
    assert.ok(definition.description.includes('widget inspection'));
    
    // Verify required parameters
    assert.ok(definition.parameters.selector);
    assert.ok(definition.parameters.includeContent);
    assert.ok(definition.parameters.includeStyling);
    assert.ok(definition.parameters.generateUUID);
  });

  test('should parse command line parameters correctly', () => {
    const argsInput = {
      args: ['--selector=.test-widget', '--includeContent=false', '--timeout=5000']
    };
    
    // Access private parseParams method via (WidgetInspectCommand as any)
    const params = (WidgetInspectCommand as any).parseParams(argsInput);
    
    assert.strictEqual(params.selector, '.test-widget');
    assert.strictEqual(params.includeContent, false);
    assert.strictEqual(params.timeout, 5000);
  });

  test('should parse object parameters correctly', () => {
    const objectInput = {
      selector: 'continuum-sidebar',
      includeContent: true,
      includeStyling: false,
      contentPreviewLength: 100
    };
    
    const params = (WidgetInspectCommand as any).parseParams(objectInput);
    
    assert.strictEqual(params.selector, 'continuum-sidebar');
    assert.strictEqual(params.includeContent, true);
    assert.strictEqual(params.includeStyling, false);
    assert.strictEqual(params.contentPreviewLength, 100);
  });

  test('should parse JSON string parameters correctly', () => {
    const jsonInput = '{"selector": "chat-widget", "generateUUID": false}';
    
    const params = (WidgetInspectCommand as any).parseParams(jsonInput);
    
    assert.strictEqual(params.selector, 'chat-widget');
    assert.strictEqual(params.generateUUID, false);
  });

  test('should create inspection script with correct structure', () => {
    const options = {
      selector: 'continuum-sidebar',
      includeContent: true,
      includeStyling: true,
      includeMetrics: true,
      contentPreviewLength: 200,
      inspectionUUID: 'test-uuid-123'
    };
    
    const script = (WidgetInspectCommand as any).createInspectionScript(options);
    
    // Verify script contains expected elements
    assert.ok(script.includes('continuum-sidebar'));
    assert.ok(script.includes('test-uuid-123'));
    assert.ok(script.includes('WIDGET_INSPECTION_START'));
    assert.ok(script.includes('WIDGET_INSPECTION_COMPLETE'));
    assert.ok(script.includes('performance.now()'));
    assert.ok(script.includes('getBoundingClientRect'));
    assert.ok(script.includes('getComputedStyle'));
  });

  test('should create minimal script when features disabled', () => {
    const options = {
      selector: 'test-widget',
      includeContent: false,
      includeStyling: false,
      includeMetrics: false,
      contentPreviewLength: 50,
      inspectionUUID: 'minimal-test'
    };
    
    const script = (WidgetInspectCommand as any).createInspectionScript(options);
    
    // Should NOT include content/styling/metrics code
    assert.ok(!script.includes('innerHTML.substring'), 'Should not include innerHTML.substring when includeContent=false');
    assert.ok(!script.includes('performance.now()'), 'Should not include performance.now() when includeMetrics=false');
    // Note: getComputedStyle might still be included for rect calculations
    
    // Should still include basic structure
    assert.ok(script.includes('test-widget'));
    assert.ok(script.includes('minimal-test'));
    assert.ok(script.includes('WIDGET_INSPECTION_START'));
  });

  test('should execute command with default parameters', async () => {
    const result = await WidgetInspectCommand.execute({});
    
    assert.strictEqual(result.success, true);
    assert.ok(result.data);
    assert.ok(result.data.inspectionUUID);
    assert.ok(result.data.inspectionUUID.startsWith('inspect-'));
    assert.ok(result.message?.includes('Widget inspection completed'));
  });

  test('should execute command with custom parameters', async () => {
    const params = {
      selector: '.custom-widget',
      includeContent: false,
      generateUUID: false
    };
    
    const result = await WidgetInspectCommand.execute(params);
    
    assert.strictEqual(result.success, true);
    assert.strictEqual(result.data.inspectionUUID, 'no-uuid');
    assert.strictEqual(result.data.selector, '.custom-widget');
    assert.strictEqual(result.data.options.includeContent, false);
  });

  test('should handle command execution errors gracefully', async () => {
    // Mock a scenario that would cause an error
    const originalCreateScript = (WidgetInspectCommand as any).createInspectionScript;
    (WidgetInspectCommand as any).createInspectionScript = () => {
      throw new Error('Test error');
    };
    
    const result = await WidgetInspectCommand.execute({});
    
    assert.strictEqual(result.success, false);
    assert.ok(result.error?.includes('Widget inspection failed'));
    assert.ok(result.data?.timestamp);
    
    // Restore original method
    (WidgetInspectCommand as any).createInspectionScript = originalCreateScript;
  });
});

console.log('🧪 Starting WidgetInspectCommand unit tests...');