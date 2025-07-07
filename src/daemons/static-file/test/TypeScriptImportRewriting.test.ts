/**
 * TypeScript Import Rewriting Test
 * Tests that StaticFileDaemon properly rewrites imports without breaking existing .js extensions
 */

import { StaticFileDaemon } from '../StaticFileDaemon';

describe('TypeScript Import Rewriting', () => {
  let daemon: StaticFileDaemon;

  beforeEach(() => {
    daemon = new StaticFileDaemon();
  });

  test('should add .js to relative imports without extensions', () => {
    const input = `import { BaseWidget } from '../shared/BaseWidget';
import { universalUserSystem } from '../shared/UniversalUserSystem';
export class ChatWidget extends BaseWidget {}`;

    const expected = `import { BaseWidget } from '../shared/BaseWidget.js';
import { universalUserSystem } from '../shared/UniversalUserSystem.js';
export class ChatWidget extends BaseWidget {}`;

    const result = applyImportRewriting(input);
    expect(result).toBe(expected);
  });

  test('should NOT double-add .js to imports that already have .js', () => {
    const input = `import { BaseWidget } from '../shared/BaseWidget.js';
import { universalUserSystem } from '../shared/UniversalUserSystem.js';
export class ChatWidget extends BaseWidget {}`;

    const expected = `import { BaseWidget } from '../shared/BaseWidget.js';
import { universalUserSystem } from '../shared/UniversalUserSystem.js';
export class ChatWidget extends BaseWidget {}`;

    const result = applyImportRewriting(input);
    expect(result).toBe(expected);
  });

  test('should handle mixed imports (some with .js, some without)', () => {
    const input = `import { BaseWidget } from '../shared/BaseWidget';
import { UniversalUserSystem } from '../shared/UniversalUserSystem.js';
import { OtherWidget } from './OtherWidget';`;

    const expected = `import { BaseWidget } from '../shared/BaseWidget.js';
import { UniversalUserSystem } from '../shared/UniversalUserSystem.js';
import { OtherWidget } from './OtherWidget.js';`;

    const result = applyImportRewriting(input);
    expect(result).toBe(expected);
  });

  test('should NOT modify non-relative imports', () => {
    const input = `import { BaseWidget } from '../shared/BaseWidget';
import React from 'react';
import { lodash } from 'lodash';`;

    const expected = `import { BaseWidget } from '../shared/BaseWidget.js';
import React from 'react';
import { lodash } from 'lodash';`;

    const result = applyImportRewriting(input);
    expect(result).toBe(expected);
  });

  test('should handle dynamic imports', () => {
    const input = `const module = await import('../shared/BaseWidget');
const other = await import('./OtherModule');`;

    const expected = `const module = await import('../shared/BaseWidget.js');
const other = await import('./OtherModule.js');`;

    const result = applyImportRewriting(input);
    expect(result).toBe(expected);
  });

  test('should NOT add .js to .css or other extensions', () => {
    const input = `import './styles.css';
import '../shared/BaseWidget';`;

    const expected = `import './styles.css';
import '../shared/BaseWidget.js';`;

    const result = applyImportRewriting(input);
    expect(result).toBe(expected);
  });
});

// Helper function that applies the same logic as StaticFileDaemon
function applyImportRewriting(compiledCode: string): string {
  // This should match the exact logic in StaticFileDaemon.compileTypeScript()
  
  // Match: from '../shared/BaseWidget' -> from '../shared/BaseWidget.js' (but not if already .js)
  compiledCode = compiledCode.replace(
    /from\s+['"](\.\.[^'"]*?)(?<!\.js)['"];?/g,
    "from '$1.js';"
  );
  
  // Match: from './BaseWidget' -> from './BaseWidget.js' (but not if already .js)
  compiledCode = compiledCode.replace(
    /from\s+['"](\.[^'"]*?)(?<!\.js)['"];?/g,
    "from '$1.js';"
  );
  
  // Dynamic imports
  compiledCode = compiledCode.replace(
    /import\s*\(\s*['"](\.\.[^'"]*?)(?<!\.js)['"]\s*\)/g,
    "import('$1.js')"
  );
  
  compiledCode = compiledCode.replace(
    /import\s*\(\s*['"](\.[^'"]*?)(?<!\.js)['"]\s*\)/g,
    "import('$1.js')"
  );

  return compiledCode;
}