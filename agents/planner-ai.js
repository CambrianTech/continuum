/**
 * PlannerAI - Strategic planning agent that loads its own tools
 */

const BaseAgent = require('./base-agent');
const WebFetchTool = require('../tools/web-fetch-tool');
const { OpenAI } = require('openai');

class PlannerAI extends BaseAgent {
  constructor() {
    super('PlannerAI');
    this.client = new OpenAI({
      apiKey: process.env.OPENAI_API_KEY,
    });
  }

  loadTools() {
    // PlannerAI loads the tools it needs
    this.tools = {
      webfetch: new WebFetchTool(),
      // Add other tools as needed
    };
  }

  async getAIResponse(task) {
    const systemPrompt = `You are PlannerAI, a strategic planning AI.

AVAILABLE TOOLS:
- WEBFETCH: https://example.com (fetches web content)

When you need to use a tool, include the exact command in your response.

Task: ${task}`;

    const completion = await this.client.chat.completions.create({
      model: "gpt-4o",
      messages: [{ role: "user", content: systemPrompt }],
      max_tokens: 1500,
      temperature: 0.7,
    });

    return completion.choices[0].message.content;
  }
}

module.exports = PlannerAI;