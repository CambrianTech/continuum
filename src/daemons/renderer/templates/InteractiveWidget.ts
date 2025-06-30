// InteractiveWidget - Common parent for widgets with user input/interaction
// Handles all common interactive widget validation and functionality

abstract class InteractiveWidget extends BaseWidget {
    protected inputElements: Map<string, HTMLInputElement> = new Map();
    protected buttonElements: Map<string, HTMLButtonElement> = new Map();

    protected validate(): void {
        super.validate();
        
        // Universal interactive widget validation
        const requiredInputs = this.getInputElements();
        for (const [id, description] of Object.entries(requiredInputs)) {
            const element = this.getTypedElement<HTMLInputElement>(id);
            if (element) {
                this.inputElements.set(id, element);
                this.log(`✅ Input element found: ${id} (${description})`);
            } else {
                this.log(`❌ Missing input element: ${id} (${description})`, 'error');
            }
        }

        const requiredButtons = this.getButtonElements();
        for (const [id, description] of Object.entries(requiredButtons)) {
            const element = this.getTypedElement<HTMLButtonElement>(id);
            if (element) {
                this.buttonElements.set(id, element);
                this.log(`✅ Button element found: ${id} (${description})`);
            } else {
                this.log(`❌ Missing button element: ${id} (${description})`, 'error');
            }
        }
        
        this.log('Interactive widget validation passed');
    }

    protected initializeWidget(): void {
        this.log('Interactive widget initializing');
        this.setupContinuumAPI();
        this.setupInteractions();
    }

    protected onApiReady(): void {
        this.enableInteractions();
    }

    /**
     * Define required input elements - override in each interactive widget
     */
    protected getInputElements(): Record<string, string> {
        return {}; // Default: no required inputs
    }

    /**
     * Define required button elements - override in each interactive widget  
     */
    protected getButtonElements(): Record<string, string> {
        return {}; // Default: no required buttons
    }

    /**
     * Setup interaction handlers - override as needed
     */
    protected setupInteractions(): void {
        // Set up common interaction patterns
        this.setupEnterKeyHandlers();
        this.setupButtonHandlers();
    }

    /**
     * Enable interactions when API is ready - override as needed
     */
    protected enableInteractions(): void {
        // Default implementation - can be overridden
        this.setInteractionsEnabled(true);
    }

    /**
     * Enable/disable all interactions
     */
    protected setInteractionsEnabled(enabled: boolean): void {
        for (const [_, input] of this.inputElements) {
            input.disabled = !enabled;
        }
        for (const [_, button] of this.buttonElements) {
            button.disabled = !enabled;
        }
    }

    /**
     * Setup Enter key handlers for all input elements
     */
    protected setupEnterKeyHandlers(): void {
        for (const [id, input] of this.inputElements) {
            input.addEventListener('keypress', async (e: KeyboardEvent) => {
                if (e.key === 'Enter' && input.value.trim()) {
                    await this.handleInput(id, input.value.trim());
                    input.value = '';
                }
            });
        }
    }

    /**
     * Setup click handlers for all button elements
     */
    protected setupButtonHandlers(): void {
        for (const [id, button] of this.buttonElements) {
            button.addEventListener('click', async () => {
                await this.handleButton(id);
            });
        }
    }

    /**
     * Handle input submission - override in each widget
     */
    protected async handleInput(inputId: string, value: string): Promise<void> {
        this.log(`Input received from ${inputId}: ${value}`);
        // Default implementation - should be overridden
    }

    /**
     * Handle button clicks - override in each widget
     */
    protected async handleButton(buttonId: string): Promise<void> {
        this.log(`Button clicked: ${buttonId}`);
        // Default implementation - should be overridden
    }

    /**
     * Safe input value retrieval
     */
    protected getInputValue(inputId: string): string {
        const input = this.inputElements.get(inputId);
        return input ? input.value.trim() : '';
    }

    /**
     * Safe input value setting
     */
    protected setInputValue(inputId: string, value: string): void {
        const input = this.inputElements.get(inputId);
        if (input) {
            input.value = value;
        }
    }

    /**
     * Focus on specific input
     */
    protected focusInput(inputId: string): void {
        const input = this.inputElements.get(inputId);
        if (input) {
            input.focus();
        }
    }
}