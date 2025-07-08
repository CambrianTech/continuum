/**
 * SidebarHeader Widget - Continuum Logo and Orb
 * Displays the continuum logo with animated orb
 * HTML content loaded from SidebarHeader.html
 * CSS loaded from SidebarHeader.css
 */
import { BaseWidget } from '../shared/BaseWidget';

export class SidebarHeader extends BaseWidget {
    constructor() {
        super();
        this.widgetName = 'SidebarHeader';
        this.widgetIcon = '🌐';
        this.widgetTitle = 'Continuum Header';
    }

    protected async initializeWidget(): Promise<void> {
        this.initializeOrb();
    }

    private initializeOrb(): void {
        // Initialize orb consciousness and animations
        this.updateOrbEmotion('calm', 'System ready');
        this.startOrbAnimations();
    }

    private updateOrbEmotion(emotion: 'calm' | 'excited' | 'focused', message: string): void {
        const orbCenter = this.shadowRoot?.querySelector('.orb-center') as HTMLElement;
        const orbEmotion = this.shadowRoot?.querySelector('.orb-emotion') as HTMLElement;
        
        if (orbCenter && orbEmotion) {
            orbCenter.setAttribute('data-emotion', emotion);
            orbCenter.setAttribute('data-message', message);
            
            const emotionSymbols = {
                calm: '●',
                excited: '✦',
                focused: '◆'
            };
            
            orbEmotion.textContent = emotionSymbols[emotion];
        }
    }

    private startOrbAnimations(): void {
        const orbGlow = this.shadowRoot?.querySelector('.orb-glow') as HTMLElement;
        if (orbGlow) {
            orbGlow.style.animation = 'breathe 4s ease-in-out infinite';
        }
    }

    // HTML content loaded from SidebarHeader.html
    // CSS loaded from SidebarHeader.css
}

// Register the custom element
if (!customElements.get('sidebar-header')) {
    customElements.define('sidebar-header', SidebarHeader);
}