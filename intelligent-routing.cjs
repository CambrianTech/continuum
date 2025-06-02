/**
 * Intelligent AI-powered routing system
 * No hardcoded case statements - uses AI to determine routing
 */

class IntelligentRouter {
  constructor(continuum) {
    this.continuum = continuum;
  }

  async route(task) {
    console.log(`🧠 AI-powered intelligent routing: ${task.substring(0, 50)}...`);
    
    // Use PlannerAI to analyze the task and determine the best approach
    const routingAnalysis = await this.continuum.sendTask('PlannerAI', `
      Analyze this task and determine the best AI routing strategy:
      
      TASK: "${task}"
      
      Consider:
      1. Does this need a specialized AI? If so, what kind?
      2. Does this need coordination between multiple AIs?
      3. Can existing AIs handle it, or should we create a new specialized one?
      
      Respond with JSON:
      {
        "approach": "single|coordination|specialized",
        "specializedAI": "NameAI" (if needed),
        "primaryAI": "PlannerAI|CodeAI|GeneralAI",
        "reasoning": "why this approach"
      }
    `);
    
    let routing;
    try {
      const jsonMatch = routingAnalysis.match(/\{[\s\S]*\}/);
      routing = jsonMatch ? JSON.parse(jsonMatch[0]) : null;
    } catch (error) {
      console.log(`⚠️  Failed to parse routing analysis, using fallback`);
      routing = { approach: "single", primaryAI: "GeneralAI", reasoning: "fallback routing" };
    }
    
    console.log(`🎯 Routing decision: ${routing.approach} - ${routing.reasoning}`);
    
    if (routing.approach === "specialized" && routing.specializedAI) {
      // Create and use specialized AI
      console.log(`🎭 Creating specialized AI: ${routing.specializedAI}`);
      const result = await this.continuum.sendTask(routing.specializedAI, task);
      return {
        role: routing.specializedAI,
        task: task,
        result: result,
        costs: this.continuum.costs,
        routing_reason: `Specialized AI created: ${routing.reasoning}`
      };
      
    } else if (routing.approach === "coordination") {
      // Coordinate multiple AIs
      console.log(`🤝 Coordinating multiple AIs...`);
      const responses = [];
      
      // PlannerAI leads coordination
      const planResponse = await this.continuum.sendTask('PlannerAI', `Lead coordination for: ${task}`);
      responses.push({ role: 'PlannerAI', type: 'coordination', result: planResponse });
      
      // CodeAI implements if needed
      if (task.toLowerCase().includes('implement') || task.toLowerCase().includes('code') || task.toLowerCase().includes('fix')) {
        const codeResponse = await this.continuum.sendTask('CodeAI', `Based on coordination from PlannerAI: ${task}`);
        responses.push({ role: 'CodeAI', type: 'implementation', result: codeResponse });
      }
      
      return {
        coordination: true,
        task: task,
        responses: responses,
        costs: this.continuum.costs,
        summary: `Multi-AI coordination: ${routing.reasoning}`
      };
      
    } else {
      // Single AI handles it
      const aiRole = routing.primaryAI || 'GeneralAI';
      console.log(`👤 Single AI routing to: ${aiRole}`);
      const result = await this.continuum.sendTask(aiRole, task);
      return {
        role: aiRole,
        task: task,
        result: result,
        costs: this.continuum.costs,
        routing_reason: routing.reasoning
      };
    }
  }
}

module.exports = IntelligentRouter;