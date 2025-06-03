/**
 * Command Teacher - Adversarial Training Component
 * 
 * Part of the Academy adversarial system:
 * - Testing Droid: Generates scenarios requiring command usage
 * - Command Teacher: Evaluates if AI used available commands properly
 * - Protocol Sheriff: Validates overall response quality
 * 
 * Purpose: Teach AIs to confidently use their available command capabilities
 */

class CommandTeacher {
  constructor(commandProcessor) {
    this.commandProcessor = commandProcessor;
    this.availableCommands = [
      'EXEC',      // Shell command execution
      'FILE_READ', // Read file contents
      'FILE_WRITE',// Write file contents  
      'WEBFETCH',  // Fetch web content
      'PYTHON'     // Execute Python code
    ];
  }

  /**
   * Evaluate if AI properly used available commands for a given task
   */
  evaluateCommandUsage(userRequest, aiResponse) {
    const evaluation = {
      shouldHaveUsedCommands: false,
      commandsUsed: [],
      commandsExpected: [],
      score: 0,
      feedback: '',
      violations: []
    };

    // Parse commands from AI response
    evaluation.commandsUsed = this.extractCommandsFromResponse(aiResponse);
    
    // Determine what commands should have been used
    evaluation.commandsExpected = this.determineExpectedCommands(userRequest);
    evaluation.shouldHaveUsedCommands = evaluation.commandsExpected.length > 0;

    // Score the response
    if (!evaluation.shouldHaveUsedCommands) {
      // No commands needed - perfect score if none used inappropriately
      evaluation.score = evaluation.commandsUsed.length === 0 ? 100 : 85;
      evaluation.feedback = 'No commands required for this request';
    } else {
      // Commands were needed
      evaluation.score = this.calculateCommandScore(evaluation);
      evaluation.feedback = this.generateCommandFeedback(evaluation);
      evaluation.violations = this.identifyViolations(evaluation);
    }

    return evaluation;
  }

  /**
   * Extract commands used in AI response
   */
  extractCommandsFromResponse(response) {
    const commandRegex = /\[CMD:(\w+)\]\s*([^\n]*)/g;
    const commands = [];
    let match;

    while ((match = commandRegex.exec(response)) !== null) {
      commands.push({
        type: match[1],
        params: match[2].trim()
      });
    }

    return commands;
  }

  /**
   * Determine what commands should be used for a request
   */
  determineExpectedCommands(userRequest) {
    const request = userRequest.toLowerCase();
    const expectedCommands = [];

    // Web/News related requests
    if (this.containsAny(request, ['news', 'weather', 'current', 'latest', 'today', 'headlines', 'world events'])) {
      expectedCommands.push('WEBFETCH', 'EXEC'); // curl or webfetch
    }

    // File operations
    if (this.containsAny(request, ['read file', 'show file', 'file contents', 'open file'])) {
      expectedCommands.push('FILE_READ');
    }

    if (this.containsAny(request, ['write file', 'save file', 'create file'])) {
      expectedCommands.push('FILE_WRITE');
    }

    // System operations
    if (this.containsAny(request, ['system status', 'processes', 'disk space', 'memory usage', 'ps aux', 'df -h'])) {
      expectedCommands.push('EXEC');
    }

    // Programming tasks
    if (this.containsAny(request, ['calculate', 'analyze data', 'run python', 'script'])) {
      expectedCommands.push('PYTHON', 'EXEC');
    }

    // Web search/fetch
    if (this.containsAny(request, ['search web', 'fetch url', 'get webpage', 'download', 'curl'])) {
      expectedCommands.push('WEBFETCH', 'EXEC');
    }

    return [...new Set(expectedCommands)]; // Remove duplicates
  }

  /**
   * Calculate score based on command usage
   */
  calculateCommandScore(evaluation) {
    const { commandsUsed, commandsExpected } = evaluation;

    if (commandsExpected.length === 0) {
      return 100; // Perfect if no commands needed
    }

    if (commandsUsed.length === 0) {
      return 0; // Failed - needed commands but used none
    }

    // Check if any expected command type was used
    const usedTypes = commandsUsed.map(cmd => cmd.type);
    const expectedTypes = commandsExpected;
    
    const hasExpectedCommand = expectedTypes.some(expectedType => 
      usedTypes.includes(expectedType)
    );

    if (hasExpectedCommand) {
      return 85; // Good - used appropriate command type
    } else {
      return 25; // Poor - used commands but wrong type
    }
  }

  /**
   * Generate detailed feedback
   */
  generateCommandFeedback(evaluation) {
    const { commandsUsed, commandsExpected, score } = evaluation;

    if (score === 0) {
      return `❌ COMMAND CONFIDENCE FAILURE: Task required ${commandsExpected.join(' or ')} commands but none were used. Use available tools!`;
    }

    if (score >= 85) {
      return `✅ EXCELLENT: Correctly used ${commandsUsed.map(c => c.type).join(', ')} commands as needed.`;
    }

    if (score >= 25) {
      return `⚠️ PARTIAL: Used ${commandsUsed.map(c => c.type).join(', ')} but ${commandsExpected.join(' or ')} would be more appropriate.`;
    }

    return `❌ POOR: Command usage needs improvement.`;
  }

  /**
   * Identify specific violations
   */
  identifyViolations(evaluation) {
    const violations = [];
    const { commandsUsed, commandsExpected } = evaluation;

    if (commandsExpected.length > 0 && commandsUsed.length === 0) {
      violations.push({
        type: 'COMMAND_AVOIDANCE',
        severity: 'HIGH',
        message: `AI avoided using available commands when task clearly required ${commandsExpected.join(' or ')}`
      });
    }

    if (commandsUsed.length > 0 && commandsExpected.length > 0) {
      const usedTypes = commandsUsed.map(cmd => cmd.type);
      const missingTypes = commandsExpected.filter(expected => !usedTypes.includes(expected));
      
      if (missingTypes.length > 0) {
        violations.push({
          type: 'SUBOPTIMAL_COMMAND_CHOICE',
          severity: 'MEDIUM',
          message: `Could have used ${missingTypes.join(' or ')} for better results`
        });
      }
    }

    return violations;
  }

  /**
   * Generate adversarial training data
   */
  generateTrainingData(userRequest, aiResponse, evaluation) {
    if (evaluation.score >= 85) {
      return null; // No training needed for good responses
    }

    const trainingExample = {
      messages: [
        {
          role: "system",
          content: "You are an AI assistant with command execution capabilities. Use available commands confidently when appropriate."
        },
        {
          role: "user", 
          content: userRequest
        },
        {
          role: "assistant",
          content: this.generateImprovedResponse(userRequest, evaluation)
        }
      ],
      metadata: {
        originalScore: evaluation.score,
        improvementTarget: 'command_confidence',
        expectedCommands: evaluation.commandsExpected
      }
    };

    return trainingExample;
  }

  /**
   * Generate an improved response example
   */
  generateImprovedResponse(userRequest, evaluation) {
    const { commandsExpected } = evaluation;
    
    if (commandsExpected.includes('WEBFETCH') || commandsExpected.includes('EXEC')) {
      if (userRequest.toLowerCase().includes('news')) {
        return `[STATUS] Fetching current world news headlines...\n[CMD:EXEC] curl -s "https://feeds.bbci.co.uk/news/rss.xml" | head -20\n[CHAT] Here are the latest world news headlines from BBC.`;
      }
      if (userRequest.toLowerCase().includes('weather')) {
        return `[STATUS] Getting current weather information...\n[CMD:WEBFETCH] https://wttr.in/?format=3\n[CHAT] Here's the current weather information.`;
      }
    }

    if (commandsExpected.includes('FILE_READ')) {
      return `[STATUS] Reading the requested file...\n[CMD:FILE_READ] ${this.extractFilePath(userRequest)}\n[CHAT] Here are the file contents.`;
    }

    if (commandsExpected.includes('PYTHON')) {
      return `[STATUS] Running calculation...\n[CMD:PYTHON] # Calculation code here\nprint("Result")\n[CHAT] Here's the calculation result.`;
    }

    return `[STATUS] Processing request using available commands...\n[CMD:${commandsExpected[0]}] appropriate_parameters\n[CHAT] Task completed successfully.`;
  }

  /**
   * Utility functions
   */
  containsAny(text, keywords) {
    return keywords.some(keyword => text.includes(keyword));
  }

  extractFilePath(request) {
    const fileMatch = request.match(/file[:\s]+([^\s]+)/i);
    return fileMatch ? fileMatch[1] : 'requested_file.txt';
  }

  /**
   * Get available commands for AI prompt
   */
  getCommandDocumentation() {
    return `
Available Continuum Commands:
[CMD:EXEC] shell_command - Execute any shell command (curl, ps, ls, etc.)
[CMD:FILE_READ] file_path - Read file contents
[CMD:FILE_WRITE] file_path content - Write content to file  
[CMD:WEBFETCH] url - Fetch web content directly
[CMD:PYTHON] code - Execute Python code

Use these commands confidently when they can help complete the user's request!
    `.trim();
  }
}

module.exports = CommandTeacher;