/**
 * Continuum Widget System - TypeScript Shared API Level Components
 * Common events, imports, and widget management
 */

// Widget System Events
export const WidgetEvents = {
    ROOM_CHANGED: 'continuum:room-changed',
    WIDGET_READY: 'continuum:widget-ready',
    BASE_READY: 'continuum:base-ready'
} as const;

export interface WidgetInterface {
    widgetName: string;
    widgetIcon: string;
    widgetTitle: string;
}

export interface RoomChangeEvent extends CustomEvent {
    detail: {
        room: string;
    };
}

export interface WidgetReadyEvent extends CustomEvent {
    detail: {
        name: string;
        widget: WidgetInterface;
    };
}

// Common Widget Management
export class WidgetSystem {
    private static widgets = new Map<string, WidgetInterface>();
    
    static register(name: string, widget: WidgetInterface): void {
        this.widgets.set(name, widget);
        document.dispatchEvent(new CustomEvent(WidgetEvents.WIDGET_READY, {
            detail: { name, widget }
        }) as WidgetReadyEvent);
    }
    
    static get(name: string): WidgetInterface | undefined {
        return this.widgets.get(name);
    }
    
    // Room management at system level
    static changeRoom(room: string): void {
        document.dispatchEvent(new CustomEvent(WidgetEvents.ROOM_CHANGED, {
            detail: { room }
        }) as RoomChangeEvent);
    }
    
    // Initialize common event handlers
    static initialize(): void {
        // Auto-wire common room change handling
        document.addEventListener(WidgetEvents.ROOM_CHANGED, (e: Event) => {
            const event = e as RoomChangeEvent;
            const room = event.detail.room;
            
            // Update room title
            const roomTitle = document.getElementById('room-title');
            if (roomTitle) {
                roomTitle.textContent = `${room.charAt(0).toUpperCase() + room.slice(1)} Chat`;
            }
            
            // Update chat widget
            const chatWidget = document.querySelector('chat-widget');
            if (chatWidget) {
                chatWidget.setAttribute('room', room);
            }
        });
        
        console.log('✅ Widget System initialized');
    }
}

// Auto-initialize when module loads
WidgetSystem.initialize();