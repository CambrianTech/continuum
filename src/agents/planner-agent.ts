/**
 * PlannerAI Agent - Clean TypeScript implementation
 * 
 * Manages its own concerns:
 * - Tools (WebFetch, FileSystem, Git)
 * - OpenAI API communication
 * - State management
 * - Input/output processing
 */

import { 
  BaseAgent, 
  IAgent, 
  AgentConfig, 
  TaskExecutionResult, 
  ToolExecutionResult,
  AgentState
} from '../interfaces/agent.interface';
import { OpenAI } from 'openai';
import { WebFetchTool } from '../tools/web-fetch-tool';
import { FileSystemTool } from '../tools/filesystem-tool';
import { GitTool } from '../tools/git-tool';

export class PlannerAgent extends BaseAgent {
  private readonly openai: OpenAI;
  private readonly systemPrompt: string;

  constructor(config: AgentConfig) {
    super(config);
    
    if (!process.env.OPENAI_API_KEY) {
      throw new Error('OPENAI_API_KEY environment variable is required');
    }
    
    this.openai = new OpenAI({
      apiKey: process.env.OPENAI_API_KEY
    });

    this.systemPrompt = `You are PlannerAI, a strategic AI agent focused on planning and coordination.

ROLE: Lead strategic planning and multi-agent coordination
PROVIDER: OpenAI GPT-4o
CAPABILITIES:
- Strategic analysis and planning
- Multi-agent coordination
- Task breakdown and delegation
- Architecture and design guidance

AVAILABLE TOOLS:
- WEBFETCH: https://example.com (fetch web content)
- FILE_READ: /path/to/file (read file contents)
- FILE_WRITE: /path/to/file [content] (write file)
- GIT_STATUS: (check git status)
- GIT_COMMIT: "message" (commit changes)

When you need to use a tool, include the exact command in your response.
Always provide strategic guidance and break down complex tasks.`;
  }

  public async initialize(): Promise<void> {
    try {
      console.log(`🔧 Initializing ${this.name}...`);
      
      // Load tools this agent needs
      this.addTool('webfetch', new WebFetchTool());
      this.addTool('filesystem', new FileSystemTool());
      this.addTool('git', new GitTool());
      
      this.setState('ready');
      console.log(`✅ ${this.name} initialized with ${this.tools.size} tools`);
    } catch (error) {
      this.setState('error');
      throw new Error(`Failed to initialize ${this.name}: ${error.message}`);
    }
  }

  public async execute(task: string): Promise<TaskExecutionResult> {
    const startTime = Date.now();
    
    try {
      this.setState('processing');
      
      // Get AI response
      const response = await this.getAIResponse(task);
      
      // Process any tool commands in the response
      const toolResults = await this.processTools(response);
      
      // Update metrics
      const duration = Date.now() - startTime;
      this.updateMetrics({
        requests: this.metrics.requests + 1,
        averageResponseTime: (this.metrics.averageResponseTime + duration) / 2
      });
      
      this.setState('ready');
      
      return {
        agent: this.name,
        task,
        response,
        toolResults: Object.freeze(toolResults),
        metrics: this.getMetrics(),
        timestamp: new Date(),
        duration
      };
      
    } catch (error) {
      this.setState('error');
      this.updateMetrics({ errors: this.metrics.errors + 1 });
      throw error;
    }
  }

  public async getAIResponse(task: string): Promise<string> {
    try {
      const completion = await this.openai.chat.completions.create({
        model: this.config.model,
        messages: [
          { role: 'system', content: this.systemPrompt },
          { role: 'user', content: `TASK: ${task}` }
        ],
        max_tokens: this.config.maxTokens,
        temperature: this.config.temperature
      });

      const response = completion.choices[0]?.message?.content;
      if (!response) {
        throw new Error('No response from OpenAI');
      }

      // Update cost metrics
      const cost = (completion.usage?.prompt_tokens || 0) * 0.005 + 
                   (completion.usage?.completion_tokens || 0) * 0.015;
      this.updateMetrics({ cost: this.metrics.cost + cost });

      return response;
      
    } catch (error) {
      throw new Error(`OpenAI API error: ${error.message}`);
    }
  }

  public async processTools(response: string): Promise<readonly ToolExecutionResult[]> {
    const results: ToolExecutionResult[] = [];
    
    // Let each tool scan for its own commands
    for (const [toolName, tool] of Array.from(this.tools)) {
      try {
        const toolResults = await tool.processResponse(response);
        results.push(...toolResults);
        
        console.log(`🔧 ${this.name} executed ${toolResults.length} ${toolName} commands`);
      } catch (error) {
        console.error(`❌ ${this.name} tool ${toolName} failed:`, error.message);
        
        results.push({
          tool: toolName,
          command: 'unknown',
          result: `Error: ${error.message}`,
          success: false,
          timestamp: new Date()
        });
      }
    }
    
    return Object.freeze(results);
  }

  public async shutdown(): Promise<void> {
    console.log(`🔄 Shutting down ${this.name}...`);
    
    try {
      // Clean up any resources
      this.tools.clear();
      this.setState('shutdown');
      
      console.log(`✅ ${this.name} shutdown complete`);
    } catch (error) {
      console.error(`❌ Error during ${this.name} shutdown:`, error.message);
    }
  }
}