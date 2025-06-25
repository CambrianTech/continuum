/**
 * DevTools Session Configuration - TypeScript-Ready Session Properties
 * ===================================================================
 * 
 * Comprehensive session configuration system with strongly-typed properties
 * for browser window management, display settings, and automation features.
 * 
 * FEATURES:
 * - Window management (size, position, visibility)
 * - Display options (headless, hidden, fullscreen)
 * - Browser behavior (debugging, security, extensions)
 * - Session isolation and resource management
 * - TypeScript-compatible property definitions
 */

/**
 * @typedef {Object} WindowProperties
 * @property {number} width - Window width in pixels
 * @property {number} height - Window height in pixels
 * @property {number} x - Window X position
 * @property {number} y - Window Y position
 * @property {boolean} maximized - Whether window should be maximized
 * @property {boolean} minimized - Whether window should be minimized
 * @property {boolean} fullscreen - Whether window should be fullscreen
 * @property {boolean} alwaysOnTop - Whether window should stay on top
 * @property {boolean} resizable - Whether window can be resized
 * @property {string} title - Window title
 */

/**
 * @typedef {Object} DisplayOptions
 * @property {boolean} headless - Run browser in headless mode
 * @property {boolean} hidden - Hide browser window
 * @property {boolean} visible - Show browser window explicitly
 * @property {number} opacity - Window opacity (0.0 to 1.0)
 * @property {string} display - Which display to show window on
 * @property {boolean} kiosk - Run in kiosk mode
 */

/**
 * @typedef {Object} BrowserOptions
 * @property {boolean} debugging - Enable debugging features
 * @property {boolean} webSecurity - Enable web security
 * @property {boolean} extensions - Allow browser extensions
 * @property {boolean} plugins - Allow browser plugins
 * @property {boolean} javascript - Enable JavaScript execution
 * @property {boolean} images - Load images
 * @property {boolean} notifications - Allow notifications
 * @property {boolean} geolocation - Allow geolocation
 * @property {boolean} camera - Allow camera access
 * @property {boolean} microphone - Allow microphone access
 * @property {string} userAgent - Custom user agent string
 * @property {Object} preferences - Browser preference overrides
 */

/**
 * @typedef {Object} SessionIsolation
 * @property {string} userDataDir - Custom user data directory
 * @property {boolean} incognito - Use incognito/private mode
 * @property {boolean} sharedSession - Share session with other instances
 * @property {boolean} persistData - Persist session data
 * @property {string} profileName - Named profile for session
 * @property {Object} cookies - Initial cookie settings
 * @property {Object} localStorage - Initial localStorage data
 */

/**
 * @typedef {Object} AutomationOptions
 * @property {boolean} autoClose - Auto-close when session ends
 * @property {number} timeout - Session timeout in milliseconds
 * @property {boolean} autoRestart - Auto-restart on crash
 * @property {boolean} captureErrors - Capture JavaScript errors
 * @property {boolean} captureConsole - Capture console messages
 * @property {boolean} captureNetwork - Capture network requests
 * @property {string} startUrl - Initial URL to navigate to
 * @property {Array<string>} preloadScripts - Scripts to inject on page load
 */

class DevToolsSessionConfig {
    constructor(purpose = 'general', aiPersona = 'system') {
        this.purpose = purpose;
        this.aiPersona = aiPersona;
        
        // Initialize with sensible defaults
        this.initializeDefaults();
    }

    /**
     * Initialize default configuration values
     */
    initializeDefaults() {
        /** @type {WindowProperties} */
        this.window = {
            width: 1280,
            height: 800,
            x: null, // Auto-position
            y: null, // Auto-position
            maximized: false,
            minimized: false,
            fullscreen: false,
            alwaysOnTop: false,
            resizable: true,
            title: 'Continuum DevTools'
        };

        /** @type {DisplayOptions} */
        this.display = {
            headless: false,
            hidden: false,
            visible: true,
            opacity: 1.0,
            display: 'primary',
            kiosk: false
        };

        /** @type {BrowserOptions} */
        this.browser = {
            debugging: true,
            webSecurity: false, // Disabled for automation
            extensions: false,
            plugins: false,
            javascript: true,
            images: true,
            notifications: false,
            geolocation: false,
            camera: false,
            microphone: false,
            userAgent: null, // Use default
            preferences: {}
        };

        /** @type {SessionIsolation} */
        this.isolation = {
            userDataDir: null, // Auto-generated
            incognito: false,
            sharedSession: true, // Use shared browser window
            persistData: false,
            profileName: null,
            cookies: {},
            localStorage: {}
        };

        /** @type {AutomationOptions} */
        this.automation = {
            autoClose: false,
            timeout: 300000, // 5 minutes
            autoRestart: true,
            captureErrors: true,
            captureConsole: true,
            captureNetwork: false,
            startUrl: 'http://localhost:9000',
            preloadScripts: []
        };

        // Session metadata
        this.metadata = {
            created: new Date(),
            sessionId: null,
            port: null,
            windowName: null
        };
    }

    /**
     * Configure window properties
     */
    setWindow(properties) {
        this.window = { ...this.window, ...properties };
        return this;
    }

    /**
     * Configure display options
     */
    setDisplay(options) {
        this.display = { ...this.display, ...options };
        return this;
    }

    /**
     * Configure browser behavior
     */
    setBrowser(options) {
        this.browser = { ...this.browser, ...options };
        return this;
    }

    /**
     * Configure session isolation
     */
    setIsolation(options) {
        this.isolation = { ...this.isolation, ...options };
        return this;
    }

    /**
     * Configure automation features
     */
    setAutomation(options) {
        this.automation = { ...this.automation, ...options };
        return this;
    }

    /**
     * Preset: Development mode with debugging enabled
     */
    development() {
        return this
            .setDisplay({ visible: true })
            .setBrowser({ 
                debugging: true, 
                webSecurity: false,
                extensions: true 
            })
            .setAutomation({ 
                captureErrors: true, 
                captureConsole: true,
                captureNetwork: true 
            });
    }

    /**
     * Preset: Production mode with minimal overhead
     */
    production() {
        return this
            .setDisplay({ headless: true })
            .setBrowser({ 
                debugging: false, 
                extensions: false,
                plugins: false 
            })
            .setAutomation({ 
                captureErrors: false, 
                captureConsole: false 
            });
    }

    /**
     * Preset: Testing mode with full isolation
     */
    testing() {
        return this
            .setIsolation({ 
                incognito: true, 
                sharedSession: false,
                persistData: false 
            })
            .setAutomation({ 
                autoClose: true,
                timeout: 60000 // 1 minute
            });
    }

    /**
     * Preset: Debugging mode with maximum visibility
     */
    debugging() {
        return this
            .setWindow({ 
                width: 1920, 
                height: 1080,
                alwaysOnTop: true 
            })
            .setDisplay({ visible: true })
            .setBrowser({ 
                debugging: true, 
                webSecurity: false 
            })
            .setAutomation({ 
                captureErrors: true, 
                captureConsole: true,
                captureNetwork: true 
            });
    }

    /**
     * Preset: Kiosk mode for presentations
     */
    kiosk() {
        return this
            .setWindow({ fullscreen: true })
            .setDisplay({ kiosk: true })
            .setBrowser({ 
                extensions: false,
                notifications: false 
            })
            .setIsolation({ incognito: true });
    }

    /**
     * Preset: Shared window mode for multiple AI personas
     */
    shared() {
        return this
            .setIsolation({ 
                sharedSession: true,
                persistData: false 
            })
            .setWindow({ 
                title: `Continuum DevTools - ${this.purpose}` 
            });
    }

    /**
     * Generate Opera browser command line arguments from configuration
     */
    toBrowserArgs(port) {
        const args = [
            `/Applications/Opera GX.app/Contents/MacOS/Opera`,
            `--remote-debugging-port=${port}`
        ];

        // Window properties
        if (!this.display.headless) {
            if (this.window.width && this.window.height) {
                args.push(`--window-size=${this.window.width},${this.window.height}`);
            }
            if (this.window.x !== null && this.window.y !== null) {
                args.push(`--window-position=${this.window.x},${this.window.y}`);
            }
            if (this.window.title) {
                args.push(`--window-name=${this.window.title}`);
            }
        }

        // Display options
        if (this.display.headless) {
            args.push('--headless');
        }
        if (this.display.kiosk) {
            args.push('--kiosk');
        }
        if (this.window.fullscreen) {
            args.push('--start-fullscreen');
        }
        if (this.window.maximized) {
            args.push('--start-maximized');
        }

        // Browser behavior
        if (!this.browser.webSecurity) {
            args.push('--disable-web-security');
        }
        if (!this.browser.extensions) {
            args.push('--disable-extensions');
        }
        if (!this.browser.plugins) {
            args.push('--disable-plugins');
        }
        if (!this.browser.notifications) {
            args.push('--disable-features=TranslateUI');
            args.push('--disable-notifications');
        }

        // Automation features
        args.push('--disable-component-update');
        args.push('--disable-background-timer-throttling');
        args.push('--disable-backgrounding-occluded-windows');
        args.push('--disable-renderer-backgrounding');
        args.push('--no-first-run');
        args.push('--no-default-browser-check');
        args.push('--disable-default-apps');

        // Session isolation
        if (this.isolation.userDataDir) {
            args.push(`--user-data-dir=${this.isolation.userDataDir}`);
        }
        if (this.isolation.incognito) {
            args.push('--incognito');
        }

        // Custom user agent
        if (this.browser.userAgent) {
            args.push(`--user-agent="${this.browser.userAgent}"`);
        }

        // Start URL
        if (this.automation.startUrl) {
            args.push(this.automation.startUrl);
        }

        return args;
    }

    /**
     * Generate user data directory path
     */
    generateUserDataDir() {
        if (this.isolation.userDataDir) {
            return this.isolation.userDataDir;
        }

        if (this.isolation.sharedSession) {
            return `/tmp/opera-devtools-continuum-shared`;
        } else {
            const sessionIdentifier = `${this.purpose}_${this.aiPersona}_${Date.now()}`;
            return `/tmp/opera-devtools-${sessionIdentifier}`;
        }
    }

    /**
     * Validate configuration
     */
    validate() {
        const errors = [];

        // Window validation
        if (this.window.width <= 0 || this.window.height <= 0) {
            errors.push('Window dimensions must be positive');
        }
        if (this.window.opacity < 0 || this.window.opacity > 1) {
            errors.push('Window opacity must be between 0 and 1');
        }

        // Automation validation
        if (this.automation.timeout <= 0) {
            errors.push('Automation timeout must be positive');
        }

        // Display validation
        if (this.display.headless && this.display.visible) {
            errors.push('Cannot be both headless and visible');
        }

        return {
            valid: errors.length === 0,
            errors: errors
        };
    }

    /**
     * Clone configuration
     */
    clone() {
        const cloned = new DevToolsSessionConfig(this.purpose, this.aiPersona);
        cloned.window = { ...this.window };
        cloned.display = { ...this.display };
        cloned.browser = { ...this.browser };
        cloned.isolation = { ...this.isolation };
        cloned.automation = { ...this.automation };
        cloned.metadata = { ...this.metadata };
        return cloned;
    }

    /**
     * Export configuration as JSON
     */
    toJSON() {
        return {
            purpose: this.purpose,
            aiPersona: this.aiPersona,
            window: this.window,
            display: this.display,
            browser: this.browser,
            isolation: this.isolation,
            automation: this.automation,
            metadata: this.metadata
        };
    }

    /**
     * Import configuration from JSON
     */
    static fromJSON(json) {
        const config = new DevToolsSessionConfig(json.purpose, json.aiPersona);
        config.window = { ...config.window, ...json.window };
        config.display = { ...config.display, ...json.display };
        config.browser = { ...config.browser, ...json.browser };
        config.isolation = { ...config.isolation, ...json.isolation };
        config.automation = { ...config.automation, ...json.automation };
        config.metadata = { ...config.metadata, ...json.metadata };
        return config;
    }
}

/**
 * Factory functions for common configurations
 */
const SessionConfigPresets = {
    /**
     * Git verification session
     */
    gitVerification() {
        return new DevToolsSessionConfig('git_verification', 'system')
            .shared()
            .debugging()
            .setWindow({ title: 'Continuum DevTools - Git Verification' });
    },

    /**
     * AI workspace session
     */
    aiWorkspace(persona) {
        return new DevToolsSessionConfig('workspace', persona)
            .shared()
            .development()
            .setWindow({ title: `Continuum DevTools - ${persona} Workspace` });
    },

    /**
     * Testing session
     */
    testing(purpose = 'test') {
        return new DevToolsSessionConfig(purpose, 'test')
            .testing()
            .setWindow({ width: 1024, height: 768 });
    },

    /**
     * Headless automation session
     */
    headless(purpose = 'automation') {
        return new DevToolsSessionConfig(purpose, 'automation')
            .production()
            .setDisplay({ headless: true });
    },

    /**
     * Presentation kiosk session
     */
    presentation() {
        return new DevToolsSessionConfig('presentation', 'display')
            .kiosk()
            .setAutomation({ autoClose: false });
    }
};

module.exports = {
    DevToolsSessionConfig,
    SessionConfigPresets
};