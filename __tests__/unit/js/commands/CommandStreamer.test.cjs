/**
 * Unit Tests for CommandStreamer
 * Tests the real-time command streaming via WebSocket
 */

const CommandStreamer = require('../../stream-commands.cjs');

// Mock fetch for testing
global.fetch = jest.fn();

describe('CommandStreamer', () => {
  let streamer;
  
  beforeEach(() => {
    streamer = new CommandStreamer('localhost', 5555);
    fetch.mockClear();
  });

  afterEach(() => {
    jest.resetAllMocks();
  });

  describe('Constructor', () => {
    test('should initialize with default host and port', () => {
      const defaultStreamer = new CommandStreamer();
      expect(defaultStreamer.baseUrl).toBe('http://localhost:5555');
      expect(defaultStreamer.commands).toEqual([]);
    });

    test('should initialize with custom host and port', () => {
      const customStreamer = new CommandStreamer('example.com', 8080);
      expect(customStreamer.baseUrl).toBe('http://example.com:8080');
    });
  });

  describe('Command Building', () => {
    test('should add basic command to stream', () => {
      streamer.add('SCREENSHOT', 'selector body');
      
      expect(streamer.commands).toHaveLength(1);
      expect(streamer.commands[0]).toEqual({
        command: 'SCREENSHOT',
        params: 'selector body',
        encoding: 'utf-8'
      });
    });

    test('should add command with custom encoding', () => {
      streamer.add('BROWSER_JS', 'Y29uc29sZS5sb2coImhlbGxvIik=', 'base64');
      
      expect(streamer.commands[0]).toEqual({
        command: 'BROWSER_JS',
        params: 'Y29uc29sZS5sb2coImhlbGxvIik=',
        encoding: 'base64'
      });
    });

    test('should add JavaScript command with automatic base64 encoding', () => {
      const jsCode = 'console.log("test");';
      streamer.addJS(jsCode);
      
      expect(streamer.commands[0]).toEqual({
        command: 'BROWSER_JS',
        params: Buffer.from(jsCode).toString('base64'),
        encoding: 'base64'
      });
    });

    test('should chain multiple commands', () => {
      streamer
        .add('SCREENSHOT', 'selector body')
        .addJS('console.log("test");')
        .add('EXEC', 'echo "hello"');
      
      expect(streamer.commands).toHaveLength(3);
      expect(streamer.commands[0].command).toBe('SCREENSHOT');
      expect(streamer.commands[1].command).toBe('BROWSER_JS');
      expect(streamer.commands[2].command).toBe('EXEC');
    });

    test('should clear command queue', () => {
      streamer.add('SCREENSHOT', 'test').add('EXEC', 'test');
      expect(streamer.commands).toHaveLength(2);
      
      streamer.clear();
      expect(streamer.commands).toHaveLength(0);
    });
  });

  describe('Streaming', () => {
    test('should stream commands successfully', async () => {
      const mockResponse = {
        ok: true,
        json: jest.fn().mockResolvedValue({
          success: true,
          command: 'SCREENSHOT',
          result: { message: 'Screenshot taken' }
        })
      };
      
      fetch.mockResolvedValue(mockResponse);
      
      streamer.add('SCREENSHOT', 'selector body');
      const results = await streamer.stream(0); // No delay for tests
      
      expect(fetch).toHaveBeenCalledWith('http://localhost:5555/connect', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          command: 'SCREENSHOT',
          params: 'selector body',
          encoding: 'utf-8'
        })
      });
      
      expect(results).toHaveLength(1);
      expect(results[0].command).toBe('SCREENSHOT');
      expect(results[0].result.success).toBe(true);
    });

    test('should handle stream errors gracefully', async () => {
      fetch.mockRejectedValue(new Error('Network error'));
      
      streamer.add('SCREENSHOT', 'test');
      const results = await streamer.stream(0);
      
      expect(results).toHaveLength(1);
      expect(results[0].error).toBe('Network error');
    });

    test('should handle HTTP errors', async () => {
      const mockResponse = {
        ok: false,
        status: 500,
        statusText: 'Internal Server Error'
      };
      
      fetch.mockResolvedValue(mockResponse);
      
      streamer.add('INVALID_COMMAND', 'test');
      const results = await streamer.stream(0);
      
      expect(results[0].error).toBe('HTTP 500: Internal Server Error');
    });

    test('should stream multiple commands in sequence', async () => {
      const mockResponse = {
        ok: true,
        json: jest.fn().mockResolvedValue({ success: true })
      };
      
      fetch.mockResolvedValue(mockResponse);
      
      streamer
        .add('SCREENSHOT', 'test1')
        .add('EXEC', 'test2')
        .addJS('console.log("test3");');
      
      const results = await streamer.stream(0);
      
      expect(fetch).toHaveBeenCalledTimes(3);
      expect(results).toHaveLength(3);
      
      // Verify correct command sequence
      const calls = fetch.mock.calls;
      expect(JSON.parse(calls[0][1].body).command).toBe('SCREENSHOT');
      expect(JSON.parse(calls[1][1].body).command).toBe('EXEC');
      expect(JSON.parse(calls[2][1].body).command).toBe('BROWSER_JS');
    });
  });

  describe('Integration Scenarios', () => {
    test('should create cyberpunk drawer fix sequence', () => {
      streamer
        .add('SCREENSHOT', 'selector body')
        .addJS(`
          const drawers = document.querySelectorAll('.slideout-panel');
          drawers.forEach(drawer => {
            drawer.style.position = 'fixed';
            drawer.style.zIndex = '9999';
          });
        `)
        .addJS(`
          const expandBtns = document.querySelectorAll('.cyber-expand-btn');
          expandBtns.forEach(btn => {
            btn.addEventListener('click', () => {
              console.log('Expand button clicked');
            });
          });
        `)
        .add('SCREENSHOT', 'selector .slideout-panel');
      
      expect(streamer.commands).toHaveLength(4);
      expect(streamer.commands[0].command).toBe('SCREENSHOT');
      expect(streamer.commands[1].command).toBe('BROWSER_JS');
      expect(streamer.commands[2].command).toBe('BROWSER_JS');
      expect(streamer.commands[3].command).toBe('SCREENSHOT');
    });

    test('should handle complex JavaScript with base64 encoding', () => {
      const complexJS = `
        // Complex drawer fix
        const drawers = document.querySelectorAll('.slideout-panel');
        console.log('🔧 Fixing', drawers.length, 'drawers');
        
        drawers.forEach((drawer, i) => {
          drawer.style.cssText = \`
            position: fixed;
            top: 0;
            left: 300px;
            width: 400px;
            height: 100vh;
            z-index: 9999;
            transform: translateX(-100%);
          \`;
        });
      `;
      
      streamer.addJS(complexJS);
      
      const command = streamer.commands[0];
      expect(command.command).toBe('BROWSER_JS');
      expect(command.encoding).toBe('base64');
      
      // Verify base64 encoding/decoding
      const decoded = Buffer.from(command.params, 'base64').toString('utf8');
      expect(decoded).toContain('slideout-panel');
      expect(decoded).toContain('z-index: 9999');
    });
  });
});