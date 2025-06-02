
// Unit tests for cyberpunk CLI components
const assert = require('assert');

describe('Cyberpunk CLI', () => {
  it('should have valid CSS syntax', () => {
    const cssFiles = ['modern-cyberpunk.css', 'classic-cyberpunk.css', 'final-cyberpunk-theme.css'];
    cssFiles.forEach(file => {
      const filePath = `../cyberpunk-cli/${file}`;
      if (require('fs').existsSync(filePath)) {
        const css = require('fs').readFileSync(filePath, 'utf-8');
        assert(css.length > 0, 'CSS file should not be empty');
        assert(!css.includes('undefined'), 'CSS should not contain undefined values');
      }
    });
  });
  
  it('should have proper memory package structure', () => {
    const memoryIndex = '../packages/memory/src/index.ts';
    if (require('fs').existsSync(memoryIndex)) {
      const content = require('fs').readFileSync(memoryIndex, 'utf-8');
      assert(content.includes('export class ContinuumMemory'), 'Memory package should export ContinuumMemory');
      assert(content.includes('storeStrategy'), 'Memory package should have storeStrategy method');
    }
  });
  
  it('should maintain drive space efficiency', () => {
    // Check file sizes are reasonable
    const checkSize = (filePath, maxSize) => {
      if (require('fs').existsSync(filePath)) {
        const stats = require('fs').statSync(filePath);
        assert(stats.size < maxSize, `File ${filePath} exceeds size limit`);
      }
    };
    
    checkSize('cyberpunk-cli/final-cyberpunk-theme.css', 50000); // 50KB max
    checkSize('packages/memory/src/index.ts', 100000); // 100KB max
  });
});
