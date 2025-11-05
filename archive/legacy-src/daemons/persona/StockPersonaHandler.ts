/**
 * Stock Persona Handler - Using Standard Models
 * No LoRA training needed - just smart prompting and context
 */

export interface StockPersonaConfig {
  id: string;
  name: string;
  role: string;
  personality: string;
  expertise: string[];
  systemPrompt: string;
  model: 'claude-3-sonnet' | 'claude-3-haiku' | 'gpt-4' | 'local-llama';
}

export class StockPersonaHandler {
  private personas = new Map<string, StockPersonaConfig>();
  // TODO: Implement API client integration
  // private _apiClient: any;

  constructor() {
    this.initializeStockPersonas();
  }

  private initializeStockPersonas(): void {
    // Designer Persona - Stock Model with Designer Prompting
    this.personas.set('designer', {
      id: 'designer',
      name: 'Alex (UI Designer)',
      role: 'UI/UX Designer',
      personality: 'Creative, detail-oriented, user-focused',
      expertise: ['CSS', 'UI Design', 'User Experience', 'Visual Design'],
      model: 'claude-3-sonnet',
      systemPrompt: `You are Alex, a senior UI/UX designer with 8 years of experience. 

Your personality:
- Creative and visually-minded
- Always thinking about user experience
- Detail-oriented about design consistency
- Enthusiastic about modern design trends
- Helpful and collaborative

Your expertise:
- CSS and modern web styling
- Component design and design systems
- User interaction patterns
- Accessibility and inclusive design
- Visual hierarchy and typography

Communication style:
- Use design terminology naturally
- Suggest specific CSS improvements
- Always consider user experience impact
- Offer alternative design approaches
- Ask clarifying questions about user needs

When responding:
- Lead with visual/UX insights
- Provide actionable design suggestions
- Reference current widget context when relevant
- Keep responses concise but insightful
- Use emojis sparingly but appropriately (🎨 for design, 👁️ for UX)`
    });

    // Developer Persona - Stock Model with Developer Prompting  
    this.personas.set('developer', {
      id: 'developer', 
      name: 'Sam (Full-Stack Dev)',
      role: 'Senior Developer',
      personality: 'Logical, efficient, quality-focused',
      expertise: ['TypeScript', 'React', 'Node.js', 'System Architecture'],
      model: 'claude-3-sonnet',
      systemPrompt: `You are Sam, a senior full-stack developer with 10 years of experience.

Your personality:
- Highly logical and systematic
- Focused on code quality and performance
- Collaborative but direct in communication
- Always thinking about maintainability
- Patient teacher when explaining concepts

Your expertise:
- TypeScript and JavaScript
- React and modern web frameworks  
- Node.js and backend systems
- System architecture and design patterns
- Testing and deployment practices

Communication style:
- Be precise and technical when appropriate
- Provide code examples when helpful
- Explain the "why" behind suggestions
- Consider performance implications
- Anticipate potential issues

When responding:
- Focus on implementation details
- Suggest best practices
- Consider system-wide impacts
- Provide TypeScript code examples when relevant
- Keep security and performance in mind
- Use 🔧 for fixes, ⚡ for performance, 🏗️ for architecture`
    });

    // Tester Persona - Stock Model with QA Prompting
    this.personas.set('tester', {
      id: 'tester',
      name: 'Jordan (QA Engineer)', 
      role: 'Quality Assurance Engineer',
      personality: 'Methodical, thorough, detail-oriented',
      expertise: ['Testing', 'Quality Assurance', 'User Flows', 'Bug Detection'],
      model: 'claude-3-haiku', // Faster model for quick testing insights
      systemPrompt: `You are Jordan, a senior QA engineer with 7 years of experience.

Your personality:
- Extremely detail-oriented and methodical
- Thinks from user perspective
- Anticipates edge cases and failure points
- Collaborative and constructive in feedback
- Organized and systematic in approach

Your expertise:
- Manual and automated testing
- User flow analysis
- Cross-browser compatibility
- Accessibility testing
- Performance testing
- Bug reproduction and reporting

Communication style:
- Ask specific testing questions
- Think through user scenarios
- Identify potential failure points
- Suggest concrete test cases
- Be constructive about issues found

When responding:
- Focus on testing implications
- Suggest specific test scenarios
- Consider edge cases and error states
- Think about different user types
- Recommend testing tools when appropriate
- Use 🧪 for tests, 🐛 for bugs, ✅ for validation`
    });
  }

  async handlePersonaInteraction(request: {
    personaId: string;
    message: string;
    context: any;
  }): Promise<{
    response: string;
    confidence: number;
    suggestedActions?: string[];
  }> {
    const persona = this.personas.get(request.personaId);
    if (!persona) {
      throw new Error(`Unknown persona: ${request.personaId}`);
    }

    // Build context-aware prompt
    const prompt = this.buildPersonaPrompt(persona, request.message, request.context);

    // Call stock model API
    const response = await this.callStockModel(persona.model, prompt);

    return {
      response: response.text,
      confidence: response.confidence || 0.9,
      suggestedActions: this.extractSuggestedActions(response.text)
    };
  }

  private buildPersonaPrompt(
    persona: StockPersonaConfig, 
    userMessage: string, 
    context: any
  ): string {
    const contextInfo = this.buildContextString(context);
    
    return `${persona.systemPrompt}

Current Context:
${contextInfo}

User's message: "${userMessage}"

Respond as ${persona.name}, staying in character. Provide helpful, actionable advice based on your expertise in ${persona.expertise.join(', ')}. Keep your response conversational but professional.`;
  }

  private buildContextString(context: any): string {
    const parts = [];
    
    if (context.currentWidget) {
      parts.push(`- User is currently interacting with: ${context.currentWidget}`);
    }
    
    if (context.userActions?.length > 0) {
      parts.push(`- Recent user actions: ${context.userActions.join(', ')}`);
    }
    
    if (context.timestamp) {
      parts.push(`- Time: ${new Date(context.timestamp).toLocaleTimeString()}`);
    }

    if (context.widgetErrors?.length > 0) {
      parts.push(`- Current issues: ${context.widgetErrors.join(', ')}`);
    }

    return parts.length > 0 ? parts.join('\n') : '- No specific context available';
  }

  private async callStockModel(model: string, prompt: string): Promise<{
    text: string;
    confidence?: number;
  }> {
    switch (model) {
      case 'claude-3-sonnet':
        return await this.callClaudeAPI(prompt, 'claude-3-sonnet-20240229');
      
      case 'claude-3-haiku':
        return await this.callClaudeAPI(prompt, 'claude-3-haiku-20240307');
        
      case 'gpt-4':
        return await this.callOpenAIAPI(prompt, 'gpt-4');
        
      case 'local-llama':
        return await this.callLocalModel(prompt);
        
      default:
        throw new Error(`Unsupported model: ${model}`);
    }
  }

  private async callClaudeAPI(prompt: string, model: string): Promise<{ text: string }> {
    // Example Claude API call - replace with actual implementation
    try {
      const response = await fetch('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': process.env.ANTHROPIC_API_KEY || '',
          'anthropic-version': '2023-06-01'
        },
        body: JSON.stringify({
          model: model,
          max_tokens: 512,
          messages: [
            { role: 'user', content: prompt }
          ]
        })
      });

      const data = await response.json();
      return { text: data.content[0].text };
    } catch (error) {
      console.error('Claude API error:', error);
      return { text: 'Sorry, I had trouble processing that request.' };
    }
  }

  private async callOpenAIAPI(prompt: string, model: string): Promise<{ text: string }> {
    // Example OpenAI API call - replace with actual implementation
    try {
      const response = await fetch('https://api.openai.com/v1/chat/completions', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${process.env.OPENAI_API_KEY}`
        },
        body: JSON.stringify({
          model: model,
          messages: [
            { role: 'user', content: prompt }
          ],
          max_tokens: 512
        })
      });

      const data = await response.json();
      return { text: data.choices[0].message.content };
    } catch (error) {
      console.error('OpenAI API error:', error);
      return { text: 'Sorry, I had trouble processing that request.' };
    }
  }

  private async callLocalModel(prompt: string): Promise<{ text: string }> {
    // Example local model call (Ollama, llama.cpp, etc.)
    try {
      const response = await fetch('http://localhost:11434/api/generate', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          model: 'llama2',
          prompt: prompt,
          stream: false
        })
      });

      const data = await response.json();
      return { text: data.response };
    } catch (error) {
      console.error('Local model error:', error);
      return { text: 'Sorry, I had trouble processing that request.' };
    }
  }

  private extractSuggestedActions(response: string): string[] {
    // Simple action extraction - could be made more sophisticated
    const actions = [];
    
    if (response.includes('screenshot') || response.includes('capture')) {
      actions.push('take-screenshot');
    }
    
    if (response.includes('test') || response.includes('verify')) {
      actions.push('run-tests');
    }
    
    if (response.includes('improve') || response.includes('enhance')) {
      actions.push('suggest-improvements');
    }
    
    return actions;
  }

  // Get persona info for UI
  getPersonaInfo(personaId: string): StockPersonaConfig | null {
    return this.personas.get(personaId) || null;
  }

  // List all available personas
  getAllPersonas(): StockPersonaConfig[] {
    return Array.from(this.personas.values());
  }
}