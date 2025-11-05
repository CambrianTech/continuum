/**
 * Critical Integration Test - TypeScript Import Rewriting
 * Tests the fix for BaseWidget and UniversalUserSystem 404 errors
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { StaticFileDaemon } from '../../StaticFileDaemon';
import { DaemonMessage } from '../../../base/DaemonProtocol';
import path from 'path';
import fs from 'fs/promises';

describe('StaticFileDaemon Import Rewriting', () => {
  let daemon: StaticFileDaemon;
  const testFilesDir = path.join(process.cwd(), 'temp-test-files');

  before(async () => {
    daemon = new StaticFileDaemon();
    await daemon.start();
    
    // Create test directory
    await fs.mkdir(testFilesDir, { recursive: true });
    
    // Create TypeScript file with relative imports (the critical test case)
    await fs.writeFile(
      path.join(testFilesDir, 'widget-with-imports.ts'),
      `import { BaseWidget } from '../shared/BaseWidget';
import { UniversalUserSystem } from '../shared/UniversalUserSystem';
import { SomeModule } from '../../shared/SomeModule';

export class TestWidget extends BaseWidget {
  private userSystem: UniversalUserSystem;
  private module: SomeModule;
  
  constructor() {
    super();
    this.userSystem = new UniversalUserSystem();
    this.module = new SomeModule();
  }
}`
    );
  });

  after(async () => {
    await daemon.stop();
    await fs.rm(testFilesDir, { recursive: true, force: true });
  });

  it('CRITICAL: should rewrite TypeScript imports to add .js extensions without creating double extensions', async () => {
    const message: DaemonMessage = {
      id: 'test-import-rewrite',
      from: 'websocket',
      to: 'static-file',
      type: 'serve_file',
      timestamp: new Date(),
      data: {
        pathname: '/temp-test-files/widget-with-imports.js',
        method: 'GET'
      }
    };

    const response = await daemon.handleMessage(message);
    
    assert.strictEqual(response.success, true, 'TypeScript compilation should succeed');
    assert.strictEqual(response.data?.contentType, 'application/javascript', 'Should return JavaScript content type');
    
    const content = response.data?.content || '';
    
    // Should add .js extensions to relative imports
    assert.ok(content.includes("from '../shared/BaseWidget.js'"), 'Should add .js to BaseWidget import');
    assert.ok(content.includes("from '../shared/UniversalUserSystem.js'"), 'Should add .js to UniversalUserSystem import');
    assert.ok(content.includes("from '../../shared/SomeModule.js'"), 'Should add .js to SomeModule import');
    
    // CRITICAL: Should NOT create double .js extensions
    assert.ok(!content.includes('.js.js'), 'Should not create double .js extensions');
    
    // Should preserve TypeScript compilation (class extends should work)
    assert.ok(content.includes('class TestWidget extends BaseWidget'), 'Should preserve class inheritance');
    assert.ok(content.includes('constructor()'), 'Should preserve constructor');
  });
});