#!/usr/bin/env node
/**
 * Safe test of the new macro command system
 * Tests command chaining without affecting the main system
 */

// Mock the command classes for testing
class MockBaseCommand {
  static parseParams(params) {
    try {
      return params ? JSON.parse(params) : {};
    } catch (e) {
      return {};
    }
  }
  
  static createSuccessResult(data, message = 'Success') {
    return {
      success: true,
      message,
      data,
      timestamp: new Date().toISOString()
    };
  }
  
  static createErrorResult(message, error = null) {
    return {
      success: false,
      message,
      error,
      timestamp: new Date().toISOString()
    };
  }
}

// Mock Screenshot Command
class MockScreenshotCommand extends MockBaseCommand {
  static async execute(params) {
    console.log('📸 Mock Screenshot: Capturing screen...');
    await new Promise(resolve => setTimeout(resolve, 100)); // Simulate async
    return this.createSuccessResult({
      filename: 'test_screenshot.png',
      path: '.continuum/screenshots/test_screenshot.png',
      size: 1024 * 50 // 50KB
    }, 'Screenshot captured');
  }
}

// Mock FindUser Command  
class MockFindUserCommand extends MockBaseCommand {
  static async execute(params) {
    const options = this.parseParams(params);
    console.log(`🔍 Mock FindUser: Searching for user...`, options);
    
    const mockUser = {
      id: 'joel',
      name: 'joel',
      role: 'admin',
      preferences: {
        mediaInput: 'slack',
        notifications: true,
        theme: 'dark'
      }
    };
    
    return this.createSuccessResult(mockUser, 'User found');
  }
}

// Mock Share Command
class MockShareCommand extends MockBaseCommand {
  static async execute(params) {
    const options = this.parseParams(params);
    console.log('🔗 Mock Share: Sharing content...', options);
    
    return this.createSuccessResult({
      shared: true,
      target: options.target,
      method: options.target?.mediaInput || 'default'
    }, 'Content shared successfully');
  }
}

// Simplified MacroCommand for testing
class TestMacroCommand extends MockBaseCommand {
  constructor(initialResult = null) {
    super();
    this.result = initialResult;
    this.pipeline = [];
  }

  // Fluent API methods
  screenshot(params = {}) {
    return this.chain('screenshot', params);
  }

  share(target) {
    return this.chain('share', { target });
  }

  findUser(query) {
    return this.chain('findUser', query);
  }

  // Core chaining method
  chain(commandName, params = {}) {
    this.pipeline.push({ command: commandName, params });
    return this; // Return this for chaining
  }

  // Execute the entire pipeline
  async execute() {
    try {
      let currentResult = this.result;
      
      console.log(`🔗 Executing test pipeline: ${this.pipeline.map(p => p.command).join(' → ')}`);

      // Execute each command in the pipeline
      for (const step of this.pipeline) {
        const { command, params: stepParams } = step;
        
        // Get the mock command class
        const CommandClass = this.getCommandClass(command);
        if (!CommandClass) {
          throw new Error(`Unknown command: ${command}`);
        }

        // Merge previous result into params if it exists
        const mergedParams = currentResult ? 
          { ...stepParams, input: currentResult } : stepParams;

        // Execute the command
        console.log(`  🔹 ${command}(${JSON.stringify(mergedParams).substring(0, 50)}...)`);
        currentResult = await CommandClass.execute(JSON.stringify(mergedParams));
        
        if (!currentResult.success) {
          throw new Error(`Command '${command}' failed: ${currentResult.message}`);
        }
        
        // Extract data for next command
        currentResult = currentResult.data || currentResult;
      }

      console.log(`✅ Test pipeline completed successfully`);
      return TestMacroCommand.createSuccessResult(currentResult, 'Test pipeline executed');

    } catch (error) {
      console.error('❌ Test pipeline failed:', error.message);
      return TestMacroCommand.createErrorResult('Test pipeline failed', error.message);
    }
  }

  // Helper to get mock command classes
  getCommandClass(commandName) {
    const commandMap = {
      'screenshot': () => MockScreenshotCommand,
      'share': () => MockShareCommand,
      'findUser': () => MockFindUserCommand
    };

    const factory = commandMap[commandName];
    return factory ? factory() : null;
  }

  // Static factory methods for fluent API
  static screenshot(params = {}) {
    return new TestMacroCommand().screenshot(params);
  }

  static findUser(query) {
    return new TestMacroCommand().findUser(query);
  }
}

// Test the fluent API
async function testFluentAPI() {
  console.log('🧪 Testing Fluent Command API\n');
  
  try {
    // Test 1: Simple screenshot
    console.log('Test 1: Simple screenshot');
    const test1 = await TestMacroCommand.screenshot().execute();
    console.log('Result:', test1.success ? '✅' : '❌', test1.message);
    console.log('');

    // Test 2: Find user
    console.log('Test 2: Find user');
    const test2 = await TestMacroCommand.findUser({name: "joel"}).execute();
    console.log('Result:', test2.success ? '✅' : '❌', test2.message);
    console.log('');

    // Test 3: Complex chain - the elegant example!
    console.log('Test 3: Complex chain - continuum.screenshot().share(continuum.findUser({name:"joel"}))');
    
    // First get the user
    const userResult = await TestMacroCommand.findUser({name: "joel"}).execute();
    const joel = userResult.data;
    
    // Then screenshot and share to that user
    const test3 = await TestMacroCommand
      .screenshot()
      .share(joel)
      .execute();
      
    console.log('Result:', test3.success ? '✅' : '❌', test3.message);
    console.log('Final data:', JSON.stringify(test3.data, null, 2));
    
  } catch (error) {
    console.error('❌ Test failed:', error);
  }
}

// Run the test
if (require.main === module) {
  testFluentAPI();
}

module.exports = { TestMacroCommand };