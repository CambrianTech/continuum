/**
 * Port Configuration - Centralized port management for Continuum
 * 
 * PRINCIPLE: One port 9000 for everything except DevTools remote debugging
 * 
 * - Port 9000: Main HTTP server, WebSocket, all UI components
 * - Port 922X: Chrome/browser DevTools remote debugging only  
 * - No other ports unless absolutely necessary for external integrations
 */

export interface PortConfiguration {
    main: {
        http: number;
        websocket: number; // Same as HTTP (WebSocket upgrade)
    };
    devtools: {
        basePort: number;
        maxPorts: number;
        range: [number, number];
    };
    reserved: {
        [purpose: string]: number;
    };
}

export const CONTINUUM_PORTS: PortConfiguration = {
    main: {
        http: 9000,
        websocket: 9000 // WebSocket uses same port via upgrade
    },
    devtools: {
        basePort: 9222,
        maxPorts: 78, // Chrome allows 9222-9299
        range: [9222, 9299]
    },
    reserved: {
        // Future: Only add if absolutely necessary
        // 'personas': 9001, // Only if personas need separate browser isolation
    }
};

/**
 * Validate port configuration prevents conflicts
 */
export function validatePortConfiguration(config: PortConfiguration): void {
    const usedPorts = new Set<number>();
    
    // Check main ports
    if (usedPorts.has(config.main.http)) {
        throw new Error(`Port conflict: HTTP port ${config.main.http} already in use`);
    }
    usedPorts.add(config.main.http);
    
    // WebSocket should use same port as HTTP
    if (config.main.websocket !== config.main.http) {
        throw new Error(`WebSocket must use same port as HTTP: ${config.main.http}`);
    }
    
    // Check DevTools range doesn't conflict with main
    const [devStart, devEnd] = config.devtools.range;
    if (config.main.http >= devStart && config.main.http <= devEnd) {
        throw new Error(`Main port ${config.main.http} conflicts with DevTools range ${devStart}-${devEnd}`);
    }
    
    // Check reserved ports
    for (const [purpose, port] of Object.entries(config.reserved)) {
        if (usedPorts.has(port)) {
            throw new Error(`Port conflict: Reserved port ${port} for ${purpose} already in use`);
        }
        if (port >= devStart && port <= devEnd) {
            throw new Error(`Reserved port ${port} for ${purpose} conflicts with DevTools range`);
        }
        usedPorts.add(port);
    }
}

/**
 * Get next available DevTools port
 */
export function getNextDevToolsPort(usedPorts: Set<number> = new Set()): number {
    const [start, end] = CONTINUUM_PORTS.devtools.range;
    
    for (let port = start; port <= end; port++) {
        if (!usedPorts.has(port)) {
            return port;
        }
    }
    
    throw new Error(`No available DevTools ports in range ${start}-${end}`);
}

/**
 * Create port allocation strategy
 */
export interface PortAllocationStrategy {
    purpose: 'main' | 'devtools' | 'reserved';
    port: number;
    description: string;
}

export function createPortAllocation(purpose: string): PortAllocationStrategy {
    const config = CONTINUUM_PORTS;
    
    switch (purpose) {
        case 'http-server':
        case 'websocket':
        case 'main-ui':
            return {
                purpose: 'main',
                port: config.main.http,
                description: `Main Continuum interface on port ${config.main.http}`
            };
            
        case 'browser-devtools':
        case 'chrome-debugging':
            const devToolsPort = getNextDevToolsPort();
            return {
                purpose: 'devtools',
                port: devToolsPort,
                description: `Browser DevTools debugging on port ${devToolsPort}`
            };
            
        default:
            // Future: Only allow if explicitly configured in reserved ports
            const reservedPort = config.reserved[purpose];
            if (reservedPort) {
                return {
                    purpose: 'reserved',
                    port: reservedPort,
                    description: `Reserved ${purpose} on port ${reservedPort}`
                };
            }
            
            throw new Error(`Unauthorized port allocation for purpose: ${purpose}. Use port 9000 or DevTools range.`);
    }
}

// Validate configuration on module load
validatePortConfiguration(CONTINUUM_PORTS);