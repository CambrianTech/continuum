/**
 * Self-Improving AI Router
 * Learns from its own successes and failures to get better over time
 */

const fs = require('fs');
const path = require('path');

class SelfImprovingRouter {
  constructor(continuum) {
    this.continuum = continuum;
    this.strategyLog = path.join(process.cwd(), 'strategies.jsonl');
    this.strategies = [];
    this.loadPreviousStrategies();
  }

  async loadPreviousStrategies() {
    try {
      if (fs.existsSync(this.strategyLog)) {
        const data = fs.readFileSync(this.strategyLog, 'utf-8');
        this.strategies = data.split('\n')
          .filter(line => line.trim())
          .map(line => JSON.parse(line));
        console.log(`📚 Loaded ${this.strategies.length} previous strategies`);
      }
    } catch (error) {
      console.log(`⚠️  Failed to load strategies: ${error.message}`);
      this.strategies = [];
    }
  }

  async route(task) {
    console.log(`🧠 Self-improving router analyzing: ${task.substring(0, 50)}...`);
    
    // Step 1: Learn from previous similar tasks
    const similarStrategies = this.findSimilarStrategies(task);
    const learnings = this.extractLearnings(similarStrategies);
    
    // Step 2: Create strategy based on learnings + AI analysis
    const strategy = await this.createStrategy(task, learnings);
    console.log(`📋 Strategy: ${strategy.description}`);
    
    // Step 3: Execute the strategy
    const startTime = Date.now();
    const result = await this.executeStrategy(strategy, task);
    const executionTime = Date.now() - startTime;
    
    // Step 4: Evaluate success and log for learning
    const success = await this.evaluateSuccess(result, task, strategy);
    await this.logStrategy(task, strategy, result, success, executionTime);
    
    console.log(`${success.successful ? '✅' : '❌'} Strategy ${success.successful ? 'succeeded' : 'failed'}: ${success.reason}`);
    
    return result;
  }

  findSimilarStrategies(task) {
    // Find strategies that worked on similar tasks
    const taskWords = task.toLowerCase().split(' ');
    const similar = this.strategies.filter(s => {
      const strategyWords = s.task.toLowerCase().split(' ');
      const overlap = taskWords.filter(word => strategyWords.includes(word)).length;
      return overlap > 1 && s.success.successful;
    });
    
    console.log(`🔍 Found ${similar.length} similar successful strategies`);
    return similar;
  }

  extractLearnings(similarStrategies) {
    if (similarStrategies.length === 0) {
      return { patterns: [], recommendations: "No previous experience with similar tasks" };
    }
    
    const patterns = {};
    similarStrategies.forEach(s => {
      const approach = s.strategy.approach;
      if (!patterns[approach]) patterns[approach] = 0;
      patterns[approach]++;
    });
    
    const bestApproach = Object.keys(patterns).reduce((a, b) => patterns[a] > patterns[b] ? a : b);
    
    return {
      patterns,
      bestApproach,
      recommendations: `Based on ${similarStrategies.length} similar tasks, ${bestApproach} approach worked best`
    };
  }

  async createStrategy(task, learnings) {
    const strategyPrompt = `
      Create a strategy for this task based on past learnings:
      
      TASK: "${task}"
      
      LEARNINGS FROM SIMILAR TASKS:
      ${learnings.recommendations}
      
      Patterns that worked: ${JSON.stringify(learnings.patterns || {})}
      
      Create a strategy and respond with JSON:
      {
        "approach": "single|coordination|specialized",
        "description": "clear description of the strategy",
        "aiRoles": ["PlannerAI", "CodeAI", etc],
        "reasoning": "why this approach based on learnings",
        "expectedOutcome": "what we expect to achieve"
      }
      
      Improve on past strategies where possible.
    `;
    
    const response = await this.continuum.sendTask('PlannerAI', strategyPrompt);
    
    try {
      const jsonMatch = response.match(/\{[\s\S]*\}/);
      const strategy = jsonMatch ? JSON.parse(jsonMatch[0]) : null;
      
      if (!strategy) throw new Error('No valid strategy JSON found');
      
      return {
        ...strategy,
        createdAt: new Date().toISOString(),
        basedOnLearnings: learnings.patterns || {}
      };
    } catch (error) {
      console.log(`⚠️  Failed to parse strategy, using fallback`);
      return {
        approach: "single",
        description: "Fallback strategy - route to GeneralAI",
        aiRoles: ["GeneralAI"],
        reasoning: "Failed to create custom strategy",
        expectedOutcome: "Basic response"
      };
    }
  }

  async executeStrategy(strategy, task) {
    console.log(`⚡ Executing strategy: ${strategy.approach}`);
    
    if (strategy.approach === "specialized" && strategy.aiRoles.length > 0) {
      const specializedRole = strategy.aiRoles[0];
      console.log(`🎭 Using specialized AI: ${specializedRole}`);
      const result = await this.continuum.sendTask(specializedRole, task);
      return {
        role: specializedRole,
        task: task,
        result: result,
        costs: this.continuum.costs,
        strategy: strategy
      };
      
    } else if (strategy.approach === "coordination") {
      console.log(`🤝 Coordinating multiple AIs: ${strategy.aiRoles.join(', ')}`);
      const responses = [];
      
      for (const role of strategy.aiRoles) {
        const result = await this.continuum.sendTask(role, task);
        responses.push({ role, type: 'coordination', result });
      }
      
      return {
        coordination: true,
        task: task,
        responses: responses,
        costs: this.continuum.costs,
        strategy: strategy
      };
      
    } else {
      // Single AI
      const role = strategy.aiRoles[0] || 'GeneralAI';
      console.log(`👤 Single AI: ${role}`);
      const result = await this.continuum.sendTask(role, task);
      return {
        role: role,
        task: task,
        result: result,
        costs: this.continuum.costs,
        strategy: strategy
      };
    }
  }

  async evaluateSuccess(result, task, strategy) {
    // Use AI to evaluate if the strategy was successful
    const evaluation = await this.continuum.sendTask('GeneralAI', `
      Evaluate if this strategy was successful:
      
      ORIGINAL TASK: "${task}"
      STRATEGY USED: ${strategy.description}
      RESULT: ${JSON.stringify(result).substring(0, 500)}...
      
      Respond with JSON:
      {
        "successful": true/false,
        "reason": "why it succeeded or failed",
        "improvements": "what could be improved next time"
      }
    `);
    
    try {
      const jsonMatch = evaluation.match(/\{[\s\S]*\}/);
      return jsonMatch ? JSON.parse(jsonMatch[0]) : { successful: false, reason: "Failed to evaluate" };
    } catch (error) {
      return { successful: false, reason: "Evaluation failed", improvements: "Fix evaluation system" };
    }
  }

  async logStrategy(task, strategy, result, success, executionTime) {
    const logEntry = {
      timestamp: new Date().toISOString(),
      task: task,
      strategy: strategy,
      result: {
        type: result.coordination ? 'coordination' : 'single',
        success: success,
        executionTime: executionTime,
        cost: this.continuum.costs.total
      },
      success: success
    };
    
    // Append to strategy log
    fs.appendFileSync(this.strategyLog, JSON.stringify(logEntry) + '\n');
    this.strategies.push(logEntry);
    
    console.log(`📝 Strategy logged for future learning`);
  }
}

module.exports = SelfImprovingRouter;