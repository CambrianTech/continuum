/**
 * Unit Tests for Command Processor
 * Tests command parsing and execution capabilities
 */

const CommandProcessor = require('../../src/core/CommandProcessor.cjs');
const fs = require('fs');
const { spawn } = require('child_process');

// Mock external dependencies
jest.mock('fs');
jest.mock('child_process');
jest.mock('node-fetch', () => jest.fn());

describe('CommandProcessor', () => {
  let processor;
  let mockFetch;

  beforeEach(() => {
    processor = new CommandProcessor();
    mockFetch = require('node-fetch');
    jest.clearAllMocks();
  });

  describe('AI Protocol Parsing', () => {
    test('should parse single command correctly', () => {
      const response = '[CMD:WEBFETCH] https://example.com\n[CHAT] Fetching data...';
      
      const parsed = processor.parseAIProtocol(response);
      
      expect(parsed.commands).toHaveLength(1);
      expect(parsed.commands[0].command).toBe('WEBFETCH');
      expect(parsed.commands[0].params).toBe('https://example.com');
      expect(parsed.chatMessage).toBe('Fetching data...');
    });

    test('should parse multiple commands', () => {
      const response = `
        [CMD:FILE_READ] package.json
        [CMD:EXEC] npm install
        [STATUS] Installing dependencies...
        [CHAT] Dependencies installed successfully.
      `;
      
      const parsed = processor.parseAIProtocol(response);
      
      expect(parsed.commands).toHaveLength(2);
      expect(parsed.commands[0].command).toBe('FILE_READ');
      expect(parsed.commands[0].params).toBe('package.json');
      expect(parsed.commands[1].command).toBe('EXEC');
      expect(parsed.commands[1].params).toBe('npm install');
      expect(parsed.statusMessage).toBe('Installing dependencies...');
      expect(parsed.chatMessage).toBe('Dependencies installed successfully.');
    });

    test('should handle responses without commands', () => {
      const response = '[CHAT] This is just a regular response without any commands.';
      
      const parsed = processor.parseAIProtocol(response);
      
      expect(parsed.commands).toHaveLength(0);
      expect(parsed.chatMessage).toBe('This is just a regular response without any commands.');
    });

    test('should handle malformed command syntax', () => {
      const response = `
        [CMD:WEBFETCH https://example.com (missing closing bracket)
        [CHAT] This should still parse the chat message.
      `;
      
      const parsed = processor.parseAIProtocol(response);
      
      expect(parsed.commands).toHaveLength(0);
      expect(parsed.chatMessage).toBe('This should still parse the chat message.');
    });
  });

  describe('WEBFETCH Command', () => {
    test('should fetch web content successfully', async () => {
      const mockResponse = {
        ok: true,
        text: jest.fn().mockResolvedValue('<html><body>Test content</body></html>')
      };
      mockFetch.mockResolvedValue(mockResponse);

      const result = await processor.webFetch('https://example.com');
      
      expect(mockFetch).toHaveBeenCalledWith('https://example.com');
      expect(result).toContain('Test content');
    });

    test('should handle fetch errors gracefully', async () => {
      mockFetch.mockRejectedValue(new Error('Network error'));

      const result = await processor.webFetch('https://invalid-url.com');
      
      expect(result).toContain('Error fetching');
      expect(result).toContain('Network error');
    });

    test('should handle non-200 responses', async () => {
      const mockResponse = {
        ok: false,
        status: 404,
        statusText: 'Not Found'
      };
      mockFetch.mockResolvedValue(mockResponse);

      const result = await processor.webFetch('https://example.com/404');
      
      expect(result).toContain('HTTP 404');
      expect(result).toContain('Not Found');
    });

    test('should clean HTML content', async () => {
      const htmlContent = `
        <html>
          <head><title>Test</title></head>
          <body>
            <script>alert('test');</script>
            <p>Important content</p>
            <style>.test { color: red; }</style>
          </body>
        </html>
      `;
      
      const mockResponse = {
        ok: true,
        text: jest.fn().mockResolvedValue(htmlContent)
      };
      mockFetch.mockResolvedValue(mockResponse);

      const result = await processor.webFetch('https://example.com');
      
      expect(result).toContain('Important content');
      expect(result).not.toContain('<script>');
      expect(result).not.toContain('<style>');
    });
  });

  describe('FILE_READ Command', () => {
    test('should read file successfully', async () => {
      const fileContent = '{"name": "test-project", "version": "1.0.0"}';
      fs.readFileSync.mockReturnValue(fileContent);

      const result = await processor.readFile('package.json');
      
      expect(fs.readFileSync).toHaveBeenCalledWith('package.json', 'utf8');
      expect(result).toBe(fileContent);
    });

    test('should handle file read errors', async () => {
      fs.readFileSync.mockImplementation(() => {
        throw new Error('File not found');
      });

      const result = await processor.readFile('nonexistent.txt');
      
      expect(result).toContain('Error reading file');
      expect(result).toContain('File not found');
    });

    test('should prevent reading sensitive files', async () => {
      const result = await processor.readFile('/etc/passwd');
      
      expect(result).toContain('Access denied');
      expect(fs.readFileSync).not.toHaveBeenCalled();
    });
  });

  describe('FILE_WRITE Command', () => {
    test('should write file successfully', async () => {
      fs.writeFileSync.mockImplementation(() => {});

      const result = await processor.writeFile('test.txt Hello, World!');
      
      expect(fs.writeFileSync).toHaveBeenCalledWith('test.txt', 'Hello, World!', 'utf8');
      expect(result).toBe('File written successfully: test.txt');
    });

    test('should handle write errors', async () => {
      fs.writeFileSync.mockImplementation(() => {
        throw new Error('Permission denied');
      });

      const result = await processor.writeFile('readonly.txt content');
      
      expect(result).toContain('Error writing file');
      expect(result).toContain('Permission denied');
    });

    test('should prevent writing to sensitive locations', async () => {
      const result = await processor.writeFile('/etc/hosts malicious content');
      
      expect(result).toContain('Access denied');
      expect(fs.writeFileSync).not.toHaveBeenCalled();
    });

    test('should parse filename and content correctly', async () => {
      fs.writeFileSync.mockImplementation(() => {});

      await processor.writeFile('config.json {"key": "value", "number": 42}');
      
      expect(fs.writeFileSync).toHaveBeenCalledWith(
        'config.json', 
        '{"key": "value", "number": 42}', 
        'utf8'
      );
    });
  });

  describe('EXEC Command', () => {
    test('should execute shell command successfully', async () => {
      const mockChild = {
        stdout: { on: jest.fn() },
        stderr: { on: jest.fn() },
        on: jest.fn()
      };

      spawn.mockReturnValue(mockChild);

      // Simulate successful execution
      setTimeout(() => {
        mockChild.stdout.on.mock.calls[0][1]('Command output\n');
        mockChild.on.mock.calls[0][1](0); // exit code 0
      }, 0);

      const result = await processor.executeShellCommand('echo "Hello World"');
      
      expect(spawn).toHaveBeenCalledWith('echo', ['"Hello World"'], { shell: true });
      expect(result).toContain('Command output');
    });

    test('should handle command failures', async () => {
      const mockChild = {
        stdout: { on: jest.fn() },
        stderr: { on: jest.fn() },
        on: jest.fn()
      };

      spawn.mockReturnValue(mockChild);

      // Simulate failed execution
      setTimeout(() => {
        mockChild.stderr.on.mock.calls[0][1]('Command not found\n');
        mockChild.on.mock.calls[0][1](1); // exit code 1
      }, 0);

      const result = await processor.executeShellCommand('invalid-command');
      
      expect(result).toContain('Command not found');
    });

    test('should prevent dangerous commands', async () => {
      const dangerousCommands = [
        'rm -rf /',
        'sudo rm -rf /*',
        'format c:',
        'dd if=/dev/zero of=/dev/sda'
      ];

      for (const cmd of dangerousCommands) {
        const result = await processor.executeShellCommand(cmd);
        
        expect(result).toContain('Command blocked for security');
        expect(spawn).not.toHaveBeenCalled();
      }
    });

    test('should timeout long-running commands', async () => {
      const mockChild = {
        stdout: { on: jest.fn() },
        stderr: { on: jest.fn() },
        on: jest.fn(),
        kill: jest.fn()
      };

      spawn.mockReturnValue(mockChild);

      // Don't call the exit handler to simulate hanging command
      const resultPromise = processor.executeShellCommand('sleep 30');
      
      // Fast-forward time to trigger timeout
      jest.advanceTimersByTime(15000);
      
      const result = await resultPromise;
      
      expect(result).toContain('Command timed out');
      expect(mockChild.kill).toHaveBeenCalledWith('SIGTERM');
    }, 10000);
  });

  describe('Command Processing Integration', () => {
    test('should process multiple commands in sequence', async () => {
      const response = `
        [CMD:FILE_READ] package.json
        [CMD:EXEC] npm test
        [CHAT] Ran tests successfully.
      `;

      // Mock file read
      fs.readFileSync.mockReturnValue('{"scripts": {"test": "jest"}}');

      // Mock command execution
      const mockChild = {
        stdout: { on: jest.fn() },
        stderr: { on: jest.fn() },
        on: jest.fn()
      };
      spawn.mockReturnValue(mockChild);
      setTimeout(() => {
        mockChild.stdout.on.mock.calls[0][1]('All tests passed\n');
        mockChild.on.mock.calls[0][1](0);
      }, 0);

      const results = await processor.processToolCommands(response);
      
      expect(results).toHaveLength(2);
      expect(results[0].tool).toBe('FILE_READ');
      expect(results[0].result).toContain('scripts');
      expect(results[1].tool).toBe('EXEC');
      expect(results[1].result).toContain('All tests passed');
    });

    test('should handle mixed success and failure', async () => {
      const response = `
        [CMD:FILE_READ] nonexistent.txt
        [CMD:WEBFETCH] https://httpbin.org/status/200
        [CHAT] Mixed results.
      `;

      // Mock file read failure
      fs.readFileSync.mockImplementation(() => {
        throw new Error('File not found');
      });

      // Mock successful web fetch
      const mockFetchResponse = {
        ok: true,
        text: jest.fn().mockResolvedValue('{"status": "ok"}')
      };
      mockFetch.mockResolvedValue(mockFetchResponse);

      const results = await processor.processToolCommands(response);
      
      expect(results).toHaveLength(2);
      expect(results[0].result).toContain('Error reading file');
      expect(results[1].result).toContain('status');
    });
  });

  describe('Security and Validation', () => {
    test('should validate command parameters', async () => {
      const maliciousCommands = [
        '[CMD:EXEC] rm -rf /',
        '[CMD:FILE_READ] /etc/shadow',
        '[CMD:WEBFETCH] file:///etc/passwd'
      ];

      for (const cmd of maliciousCommands) {
        const results = await processor.processToolCommands(cmd + '\n[CHAT] Testing');
        
        expect(results[0].result).toContain('denied');
      }
    });

    test('should sanitize file paths', async () => {
      const maliciousPaths = [
        '../../../etc/passwd',
        '..\\..\\windows\\system32\\drivers\\etc\\hosts',
        '/proc/version'
      ];

      for (const path of maliciousPaths) {
        const result = await processor.readFile(path);
        
        expect(result).toContain('Access denied');
      }
    });
  });
});