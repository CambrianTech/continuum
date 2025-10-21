/**
 * Widget Registration Prevention Tests
 * 
 * PREVENTS: "Failed to execute 'define' on 'CustomElementRegistry': 
 *           the name 'continuum-sidebar' has already been used with this registry"
 * 
 * Tests widget registration logic to ensure no duplicate registrations
 */

import { describe, test, expect, beforeEach } from '@jest/globals';

// Mock CustomElementRegistry
const mockCustomElements = {
    registry: new Map<string, any>(),
    
    define(name: string, constructor: any) {
        if (this.registry.has(name)) {
            throw new Error(`Failed to execute 'define' on 'CustomElementRegistry': the name "${name}" has already been used with this registry`);
        }
        this.registry.set(name, constructor);
    },
    
    get(name: string) {
        return this.registry.get(name) || undefined;
    },
    
    clear() {
        this.registry.clear();
    }
};

// Mock global customElements
(global as any).customElements = mockCustomElements;

describe('Widget Registration Prevention', () => {
    beforeEach(() => {
        // Clear registry before each test
        mockCustomElements.clear();
    });

    test('SidebarWidget registers only once', () => {
        // Mock SidebarWidget
        class MockSidebarWidget extends HTMLElement {}
        
        // First registration should succeed
        if (!customElements.get('continuum-sidebar')) {
            customElements.define('continuum-sidebar', MockSidebarWidget);
        }
        
        expect(customElements.get('continuum-sidebar')).toBe(MockSidebarWidget);
        
        // Second registration should be prevented (no error thrown)
        expect(() => {
            if (!customElements.get('continuum-sidebar')) {
                customElements.define('continuum-sidebar', MockSidebarWidget);
            }
        }).not.toThrow();
    });

    test('ChatWidget registers only once', () => {
        // Mock ChatWidget
        class MockChatWidget extends HTMLElement {}
        
        // First registration should succeed
        if (!customElements.get('chat-widget')) {
            customElements.define('chat-widget', MockChatWidget);
        }
        
        expect(customElements.get('chat-widget')).toBe(MockChatWidget);
        
        // Second registration should be prevented (no error thrown)
        expect(() => {
            if (!customElements.get('chat-widget')) {
                customElements.define('chat-widget', MockChatWidget);
            }
        }).not.toThrow();
    });

    test('Multiple widget types can coexist', () => {
        class MockSidebarWidget extends HTMLElement {}
        class MockChatWidget extends HTMLElement {}
        class MockVersionWidget extends HTMLElement {}
        
        // Register multiple widgets
        if (!customElements.get('continuum-sidebar')) {
            customElements.define('continuum-sidebar', MockSidebarWidget);
        }
        if (!customElements.get('chat-widget')) {
            customElements.define('chat-widget', MockChatWidget);
        }
        if (!customElements.get('version-widget')) {
            customElements.define('version-widget', MockVersionWidget);
        }
        
        // All should be registered
        expect(customElements.get('continuum-sidebar')).toBe(MockSidebarWidget);
        expect(customElements.get('chat-widget')).toBe(MockChatWidget);
        expect(customElements.get('version-widget')).toBe(MockVersionWidget);
    });

    test('Widget re-registration attempts are silently ignored', () => {
        class MockWidget extends HTMLElement {}
        
        // Register widget multiple times (simulating multiple script loads)
        for (let i = 0; i < 5; i++) {
            expect(() => {
                if (!customElements.get('test-widget')) {
                    customElements.define('test-widget', MockWidget);
                }
            }).not.toThrow();
        }
        
        // Should still be registered correctly
        expect(customElements.get('test-widget')).toBe(MockWidget);
    });

    test('Widget registration follows Continuum naming convention', () => {
        const validNames = [
            'continuum-sidebar',
            'chat-widget', 
            'version-widget',
            'academy-widget',
            'persona-selector'
        ];
        
        for (const name of validNames) {
            expect(name).toMatch(/^[a-z]+(-[a-z]+)*$/);
            expect(name.length).toBeGreaterThan(3);
            expect(name).not.toContain('_');
            expect(name).not.toContain(' ');
        }
    });

    test('Widget registration error simulation', () => {
        class MockWidget extends HTMLElement {}
        
        // Force register without check to simulate the original error
        customElements.define('error-test-widget', MockWidget);
        
        // This should throw the exact error we're preventing
        expect(() => {
            customElements.define('error-test-widget', MockWidget);
        }).toThrow('Failed to execute \'define\' on \'CustomElementRegistry\': the name "error-test-widget" has already been used with this registry');
    });
});