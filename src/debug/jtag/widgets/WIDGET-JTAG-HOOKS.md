# Widget JTAG Development Hooks

## 🎯 **BUILT-IN JTAG INTEGRATION**

### **Vision Statement**
Widgets should have native JTAG hooks for common development tasks - animation testing, theme switching, state capture, and visual debugging - all using the existing event and command subsystem internally but exposed as simple widget methods.

## 🔧 **Widget Development API Integration**

### **Enhanced WidgetBase with JTAG Hooks**
```typescript
// widgets/shared/WidgetBase.ts
export abstract class WidgetBase {
  protected jtagHooks: WidgetJTAGHooks;
  
  constructor(context: WidgetContext) {
    super();
    this.context = context;
    this.jtagHooks = new WidgetJTAGHooks(this, context);
  }
  
  // === BUILT-IN DEVELOPMENT HOOKS ===
  
  // Animation testing hooks
  async captureBeforeAnimation(animationName: string): Promise<string> {
    return this.jtagHooks.captureBeforeAnimation(animationName);
  }
  
  async captureAfterAnimation(animationName: string): Promise<string> {
    return this.jtagHooks.captureAfterAnimation(animationName);
  }
  
  async testAnimation(animationName: string, trigger: () => Promise<void>): Promise<AnimationTestResult> {
    return this.jtagHooks.testAnimation(animationName, trigger);
  }
  
  // Theme testing hooks
  async captureAllThemes(): Promise<ThemeCaptureResult> {
    return this.jtagHooks.captureAllThemes();
  }
  
  async compareThemes(themes: string[]): Promise<ThemeComparisonResult> {
    return this.jtagHooks.compareThemes(themes);
  }
  
  // State capture hooks
  async captureCurrentState(stateName?: string): Promise<string> {
    return this.jtagHooks.captureCurrentState(stateName);
  }
  
  async captureStateSequence(states: StateSequence[]): Promise<StateSequenceResult> {
    return this.jtagHooks.captureStateSequence(states);
  }
  
  // Live debugging hooks
  async liveEditCSS(property: string, value: string): Promise<void> {
    return this.jtagHooks.liveEditCSS(property, value);
  }
  
  async inspectThemeTokens(): Promise<ThemeTokens> {
    return this.jtagHooks.inspectThemeTokens();
  }
  
  // Event simulation hooks
  async simulateUserInteraction(interaction: UserInteraction): Promise<InteractionResult> {
    return this.jtagHooks.simulateUserInteraction(interaction);
  }
}
```

## 🎬 **Animation Development Hooks**

### **Animation Testing Integration**
```typescript
// widgets/shared/hooks/WidgetJTAGHooks.ts
export class WidgetJTAGHooks {
  constructor(
    private widget: WidgetBase,
    private context: WidgetContext
  ) {}
  
  async testAnimation(animationName: string, trigger: () => Promise<void>): Promise<AnimationTestResult> {
    const widgetSelector = this.widget.getSelector();
    const baseFilename = `${this.widget.constructor.name.toLowerCase()}-${animationName}`;
    
    // Capture before state
    const beforeFilename = `${baseFilename}-before.png`;
    await this.context.executeCommand('screenshot', {
      querySelector: widgetSelector,
      filename: beforeFilename
    });
    
    console.log(`🎬 ${this.widget.constructor.name}: Captured before state for ${animationName}`);
    
    // Execute the animation trigger
    await trigger();
    
    // Wait for animation to complete (smart timing based on CSS custom properties)
    const animationDuration = await this.getAnimationDuration(animationName);
    await this.wait(animationDuration + 50); // Small buffer
    
    // Capture after state
    const afterFilename = `${baseFilename}-after.png`;
    await this.context.executeCommand('screenshot', {
      querySelector: widgetSelector,
      filename: afterFilename
    });
    
    console.log(`✅ ${this.widget.constructor.name}: Animation test complete for ${animationName}`);
    
    return {
      animationName,
      beforeImage: beforeFilename,
      afterImage: afterFilename,
      duration: animationDuration,
      success: true
    };
  }
  
  private async getAnimationDuration(animationName: string): Promise<number> {
    const result = await this.context.executeCommand('exec', {
      code: `
        const element = document.querySelector('${this.widget.getSelector()}');
        const computedStyle = getComputedStyle(element);
        
        // Try to get specific animation duration, fallback to normal
        const customDuration = computedStyle.getPropertyValue('--animation-duration-${animationName}');
        const normalDuration = computedStyle.getPropertyValue('--animation-duration-normal');
        const intensity = parseFloat(computedStyle.getPropertyValue('--animation-intensity') || '1');
        
        const duration = customDuration || normalDuration || '300ms';
        const ms = parseFloat(duration) * (duration.includes('s') && !duration.includes('ms') ? 1000 : 1);
        
        return Math.round(ms * intensity);
      `,
      environment: 'browser'
    });
    
    return result || 300;
  }
}
```

### **Widget Animation Usage Examples**
```typescript
// widgets/chat-widget/shared/ChatWidget.ts
export class ChatWidget extends WidgetBase {
  
  // Development method - automatically available in all widgets
  async testPanelSlideAnimation(): Promise<void> {
    await this.testAnimation('panel-slide', async () => {
      // Trigger the animation
      this.shadowRoot.querySelector('.chat-panel').classList.add('animate-slide-in');
    });
  }
  
  async testAllChatAnimations(): Promise<void> {
    console.log('🧪 Testing all chat widget animations...');
    
    // Test message appearance
    await this.testAnimation('message-appear', async () => {
      const messageElement = this.createMessageElement('Test message');
      this.shadowRoot.querySelector('.message-list').appendChild(messageElement);
    });
    
    // Test typing indicator
    await this.testAnimation('typing-indicator', async () => {
      this.showTypingIndicator();
    });
    
    // Test panel expansion
    await this.testAnimation('panel-expand', async () => {
      this.expandChatPanel();
    });
    
    console.log('✅ All chat animations tested');
  }
}
```

## 🎨 **Theme Development Hooks**

### **Multi-Theme Capture System**
```typescript
export class WidgetJTAGHooks {
  async captureAllThemes(): Promise<ThemeCaptureResult> {
    const themes = ['basic', 'cyberpunk', 'anime'];
    const captures: Record<string, string> = {};
    const widgetName = this.widget.constructor.name.toLowerCase();
    
    for (const theme of themes) {
      console.log(`🎨 Capturing ${widgetName} in ${theme} theme...`);
      
      // Switch theme
      await this.context.executeCommand('exec', {
        code: `document.documentElement.setAttribute('data-theme', '${theme}')`,
        environment: 'browser'
      });
      
      // Wait for theme transition
      await this.wait(100);
      
      // Capture widget in this theme
      const filename = `${widgetName}-theme-${theme}.png`;
      await this.context.executeCommand('screenshot', {
        querySelector: this.widget.getSelector(),
        filename
      });
      
      captures[theme] = filename;
    }
    
    console.log(`🎨 All themes captured for ${widgetName}`);
    return {
      widget: widgetName,
      themes: captures,
      timestamp: Date.now()
    };
  }
  
  async compareThemes(themes: string[]): Promise<ThemeComparisonResult> {
    const captures = await this.captureThemeSubset(themes);
    
    // Generate comparison HTML (could be expanded to actual image diff)
    const comparisonHtml = this.generateThemeComparisonHTML(captures);
    
    await this.context.executeCommand('fileSave', {
      filename: `${this.widget.constructor.name.toLowerCase()}-theme-comparison.html`,
      content: comparisonHtml
    });
    
    return {
      themes,
      captures,
      comparisonFile: `${this.widget.constructor.name.toLowerCase()}-theme-comparison.html`
    };
  }
}
```

## 🔄 **State Sequence Testing**

### **User Flow Capture System**
```typescript
export class WidgetJTAGHooks {
  async captureStateSequence(sequence: StateSequence[]): Promise<StateSequenceResult> {
    const results: StateResult[] = [];
    const widgetName = this.widget.constructor.name.toLowerCase();
    
    for (let i = 0; i < sequence.length; i++) {
      const state = sequence[i];
      console.log(`📸 Capturing state: ${state.name}`);
      
      // Execute state change action
      if (state.action) {
        await state.action();
      }
      
      // Wait for state to settle
      await this.wait(state.waitMs || 200);
      
      // Capture state
      const filename = `${widgetName}-sequence-${i.toString().padStart(2, '0')}-${state.name}.png`;
      await this.context.executeCommand('screenshot', {
        querySelector: this.widget.getSelector(),
        filename
      });
      
      results.push({
        stepNumber: i,
        stateName: state.name,
        filename,
        timestamp: Date.now()
      });
    }
    
    // Generate sequence HTML report
    const reportHtml = this.generateSequenceReport(results);
    await this.context.executeCommand('fileSave', {
      filename: `${widgetName}-state-sequence-report.html`,
      content: reportHtml
    });
    
    return {
      widget: widgetName,
      sequence: results,
      reportFile: `${widgetName}-state-sequence-report.html`
    };
  }
}

// Usage in widget development
export class AcademyTrainerWidget extends WidgetBase {
  async testTrainingFlowSequence(): Promise<void> {
    await this.captureStateSequence([
      {
        name: 'initial-state',
        action: async () => { /* Widget starts in default state */ }
      },
      {
        name: 'persona-selected',
        action: async () => this.selectPersona('claude'),
        waitMs: 300
      },
      {
        name: 'lora-configured',
        action: async () => this.configureLORASettings({ strength: 0.8 }),
        waitMs: 200
      },
      {
        name: 'training-started',
        action: async () => this.startTraining(),
        waitMs: 500
      },
      {
        name: 'progress-showing',
        action: async () => this.simulateTrainingProgress(50),
        waitMs: 200
      },
      {
        name: 'training-complete',
        action: async () => this.simulateTrainingComplete(),
        waitMs: 300
      }
    ]);
  }
}
```

## 🔍 **Live CSS Development Hooks**

### **Real-Time Style Editing**
```typescript
export class WidgetJTAGHooks {
  async liveEditCSS(property: string, value: string): Promise<void> {
    const widgetName = this.widget.constructor.name;
    console.log(`🎨 ${widgetName}: Live editing ${property} = ${value}`);
    
    await this.context.executeCommand('exec', {
      code: `
        // Apply to document root for theme tokens
        document.documentElement.style.setProperty('${property}', '${value}');
        
        // Also apply directly to widget if it has a custom property
        const widget = document.querySelector('${this.widget.getSelector()}');
        if (widget && widget.style) {
          widget.style.setProperty('${property}', '${value}');
        }
        
        console.log('🔧 CSS property updated: ${property} = ${value}');
      `,
      environment: 'browser'
    });
  }
  
  async liveEditAndCapture(property: string, values: string[]): Promise<LiveEditResult[]> {
    const results: LiveEditResult[] = [];
    const widgetName = this.widget.constructor.name.toLowerCase();
    
    for (let i = 0; i < values.length; i++) {
      const value = values[i];
      
      // Apply CSS change
      await this.liveEditCSS(property, value);
      
      // Wait for rendering
      await this.wait(100);
      
      // Capture result
      const filename = `${widgetName}-live-edit-${property.replace('--', '')}-${i}.png`;
      await this.context.executeCommand('screenshot', {
        querySelector: this.widget.getSelector(),
        filename
      });
      
      results.push({
        property,
        value,
        filename,
        step: i
      });
    }
    
    return results;
  }
  
  // Usage: Test different animation durations
  async testAnimationTimings(): Promise<void> {
    await this.liveEditAndCapture('--animation-duration-normal', [
      '150ms',
      '300ms', 
      '500ms',
      '800ms'
    ]);
  }
}
```

## 🎮 **User Interaction Simulation**

### **Programmatic Interaction Testing**
```typescript
export class WidgetJTAGHooks {
  async simulateUserInteraction(interaction: UserInteraction): Promise<InteractionResult> {
    const startTime = Date.now();
    
    // Capture before interaction
    const beforeFilename = `interaction-${interaction.type}-before.png`;
    await this.captureCurrentState(beforeFilename);
    
    // Execute interaction
    await this.context.executeCommand('exec', {
      code: this.generateInteractionCode(interaction),
      environment: 'browser'
    });
    
    // Wait for interaction effects
    await this.wait(interaction.waitMs || 300);
    
    // Capture after interaction  
    const afterFilename = `interaction-${interaction.type}-after.png`;
    await this.captureCurrentState(afterFilename);
    
    const duration = Date.now() - startTime;
    
    return {
      interaction: interaction.type,
      beforeImage: beforeFilename,
      afterImage: afterFilename,
      duration,
      success: true
    };
  }
  
  private generateInteractionCode(interaction: UserInteraction): string {
    switch (interaction.type) {
      case 'click':
        return `
          const element = document.querySelector('${this.widget.getSelector()} ${interaction.selector}');
          if (element) {
            element.click();
            console.log('🖱️ Simulated click on:', '${interaction.selector}');
          }
        `;
      
      case 'hover':
        return `
          const element = document.querySelector('${this.widget.getSelector()} ${interaction.selector}');
          if (element) {
            element.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
            console.log('👆 Simulated hover on:', '${interaction.selector}');
          }
        `;
      
      case 'type':
        return `
          const element = document.querySelector('${this.widget.getSelector()} ${interaction.selector}');
          if (element && element.tagName === 'INPUT') {
            element.value = '${interaction.value}';
            element.dispatchEvent(new Event('input', { bubbles: true }));
            console.log('⌨️ Simulated typing:', '${interaction.value}');
          }
        `;
      
      default:
        return `console.log('Unknown interaction type: ${interaction.type}');`;
    }
  }
}

// Usage in widget development
export class ChatWidget extends WidgetBase {
  async testUserInteractions(): Promise<void> {
    // Test message input
    await this.simulateUserInteraction({
      type: 'type',
      selector: '.message-input',
      value: 'Hello, this is a test message!',
      waitMs: 200
    });
    
    // Test send button click
    await this.simulateUserInteraction({
      type: 'click', 
      selector: '.send-button',
      waitMs: 500
    });
    
    // Test hover effects on messages
    await this.simulateUserInteraction({
      type: 'hover',
      selector: '.message-bubble:last-child',
      waitMs: 200
    });
  }
}
```

## 🔧 **Widget Development Workflow Integration**

### **Complete Development Testing Suite**
```typescript
// widgets/shared/WidgetDevTestSuite.ts
export class WidgetDevTestSuite {
  static async runCompleteDevelopmentSuite(widget: WidgetBase): Promise<DevelopmentTestReport> {
    const report: DevelopmentTestReport = {
      widgetName: widget.constructor.name,
      timestamp: Date.now(),
      results: {}
    };
    
    console.log(`🧪 Running complete development suite for ${widget.constructor.name}...`);
    
    // 1. Theme testing
    console.log('📸 Testing themes...');
    report.results.themes = await widget.captureAllThemes();
    
    // 2. Animation testing (if widget has animations)
    if (widget.hasAnimations) {
      console.log('🎬 Testing animations...');
      report.results.animations = await widget.testAllAnimations();
    }
    
    // 3. State sequence testing (if widget has state flows)
    if (widget.hasStateSequences) {
      console.log('🔄 Testing state sequences...');
      report.results.stateSequences = await widget.testAllStateSequences();
    }
    
    // 4. Interaction testing
    console.log('🎮 Testing user interactions...');
    report.results.interactions = await widget.testAllInteractions();
    
    // 5. Responsive testing (different viewport sizes)
    console.log('📱 Testing responsive behavior...');
    report.results.responsive = await widget.testResponsiveStates();
    
    // Generate comprehensive report
    const reportHtml = this.generateComprehensiveReport(report);
    await widget.context.executeCommand('fileSave', {
      filename: `${widget.constructor.name.toLowerCase()}-development-report.html`,
      content: reportHtml
    });
    
    console.log(`✅ Complete development suite finished for ${widget.constructor.name}`);
    return report;
  }
}

// Usage during widget development
export async function developChatWidget(): Promise<void> {
  // Create and initialize widget
  const chatWidget = new ChatWidget(context);
  await chatWidget.initialize();
  
  // Run complete development testing
  const report = await WidgetDevTestSuite.runCompleteDevelopmentSuite(chatWidget);
  
  console.log('🎉 Chat widget development testing complete!');
  console.log('📊 Report generated:', report);
}
```

## 🎯 **Built-in Hook Types**

### **TypeScript Interface Definitions**
```typescript
// widgets/shared/types/JTAGHookTypes.ts
export interface AnimationTestResult {
  animationName: string;
  beforeImage: string;
  afterImage: string;
  duration: number;
  success: boolean;
}

export interface StateSequence {
  name: string;
  action?: () => Promise<void>;
  waitMs?: number;
}

export interface UserInteraction {
  type: 'click' | 'hover' | 'type' | 'scroll';
  selector: string;
  value?: string;
  waitMs?: number;
}

export interface ThemeCaptureResult {
  widget: string;
  themes: Record<string, string>;
  timestamp: number;
}

export interface DevelopmentTestReport {
  widgetName: string;
  timestamp: number;
  results: {
    themes?: ThemeCaptureResult;
    animations?: AnimationTestResult[];
    stateSequences?: StateSequenceResult[];
    interactions?: InteractionResult[];
    responsive?: ResponsiveTestResult[];
  };
}
```

This creates a powerful, integrated development environment where widgets automatically have sophisticated testing capabilities built-in, all leveraging the existing JTAG event and command subsystem under the hood. Developers get visual feedback, automated testing, and comprehensive reports without having to manually write JTAG commands.