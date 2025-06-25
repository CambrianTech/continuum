/**
 * Session Configuration Builder - TypeScript-Ready OOP Design
 * =========================================================== 
 * 
 * Fluent builder pattern implementation for DevTools session configuration.
 * Follows TypeScript interfaces and provides method chaining with validation.
 * Implements clean OOP principles with inheritance and composition.
 */

const { DevToolsSessionConfig } = require('./DevToolsSessionConfig.cjs');

// Enums (will be replaced by TypeScript enums in .ts version)
const SessionPurpose = {
    GIT_VERIFICATION: 'git_verification',
    WORKSPACE: 'workspace',
    TESTING: 'testing', 
    AUTOMATION: 'automation',
    PRESENTATION: 'presentation',
    DEBUGGING: 'debugging',
    DEVELOPMENT: 'development'
};

const WindowState = {
    NORMAL: 'normal',
    MINIMIZED: 'minimized',
    MAXIMIZED: 'maximized',
    FULLSCREEN: 'fullscreen',
    HIDDEN: 'hidden'
};

const BrowserEngine = {
    OPERA_GX: 'opera_gx',
    CHROME: 'chrome',
    CHROMIUM: 'chromium',
    EDGE: 'edge'
};

const ScreenshotFormat = {
    PNG: 'png',
    JPEG: 'jpeg',
    WEBP: 'webp'
};

/**
 * Abstract base class for all configuration components
 * Implements common validation and serialization patterns
 */
class BaseConfigComponent {
    constructor() {
        this._data = {};
        this._validators = new Set();
    }

    /**
     * Add validation rule
     */
    addValidator(validator) {
        this._validators.add(validator);
        return this;
    }

    /**
     * Validate configuration
     */
    validate() {
        const errors = [];
        for (const validator of this._validators) {
            const result = validator(this._data);
            if (result !== true) {
                errors.push(result);
            }
        }
        return {
            valid: errors.length === 0,
            errors: errors
        };
    }

    /**
     * Get immutable configuration data
     */
    getData() {
        return Object.freeze({ ...this._data });
    }

    /**
     * Merge data with validation
     */
    mergeData(newData) {
        this._data = { ...this._data, ...newData };
        return this;
    }
}

/**
 * Window properties configuration
 * Handles window size, position, and display state
 */
class WindowConfig extends BaseConfigComponent {
    constructor() {
        super();
        this._data = {
            width: 1280,
            height: 800,
            x: null,
            y: null,
            state: WindowState.NORMAL,
            alwaysOnTop: false,
            resizable: true,
            title: 'Continuum DevTools',
            opacity: 1.0
        };

        // Add validators
        this.addValidator((data) => {
            if (data.width <= 0 || data.height <= 0) {
                return 'Window dimensions must be positive';
            }
            return true;
        });

        this.addValidator((data) => {
            if (data.opacity < 0 || data.opacity > 1) {
                return 'Window opacity must be between 0 and 1';
            }
            return true;
        });
    }

    setDimensions(width, height) {
        return this.mergeData({ width, height });
    }

    setPosition(x, y) {
        return this.mergeData({ x, y });
    }

    setState(state) {
        if (!Object.values(WindowState).includes(state)) {
            throw new Error(`Invalid window state: ${state}`);
        }
        return this.mergeData({ state });
    }

    setAlwaysOnTop(alwaysOnTop = true) {
        return this.mergeData({ alwaysOnTop });
    }

    setResizable(resizable = true) {
        return this.mergeData({ resizable });
    }

    setTitle(title) {
        return this.mergeData({ title });
    }

    setOpacity(opacity) {
        return this.mergeData({ opacity });
    }

    // Convenience methods
    maximize() {
        return this.setState(WindowState.MAXIMIZED);
    }

    minimize() {
        return this.setState(WindowState.MINIMIZED);
    }

    fullscreen() {
        return this.setState(WindowState.FULLSCREEN);
    }

    hide() {
        return this.setState(WindowState.HIDDEN);
    }
}

/**
 * Display options configuration
 * Manages headless mode, visibility, and monitor settings
 */
class DisplayConfig extends BaseConfigComponent {
    constructor() {
        super();
        this._data = {
            headless: false,
            visible: true,
            kiosk: false,
            displayId: 'primary',
            multiMonitor: false
        };

        this.addValidator((data) => {
            if (data.headless && data.visible) {
                return 'Cannot be both headless and visible';
            }
            return true;
        });
    }

    setHeadless(headless = true) {
        return this.mergeData({ headless, visible: !headless });
    }

    setVisible(visible = true) {
        return this.mergeData({ visible, headless: !visible });
    }

    setKiosk(kiosk = true) {
        return this.mergeData({ kiosk });
    }

    setDisplayId(displayId) {
        return this.mergeData({ displayId });
    }

    setMultiMonitor(multiMonitor = true) {
        return this.mergeData({ multiMonitor });
    }
}

/**
 * Browser behavior configuration
 * Controls browser engine, security, and feature settings
 */
class BrowserConfig extends BaseConfigComponent {
    constructor() {
        super();
        this._data = {
            engine: BrowserEngine.OPERA_GX,
            debugging: true,
            webSecurity: false,
            extensions: false,
            plugins: false,
            javascript: true,
            images: true,
            notifications: false,
            userAgent: null,
            preferences: {}
        };
    }

    setEngine(engine) {
        if (!Object.values(BrowserEngine).includes(engine)) {
            throw new Error(`Invalid browser engine: ${engine}`);
        }
        return this.mergeData({ engine });
    }

    setDebugging(debugging = true) {
        return this.mergeData({ debugging });
    }

    setWebSecurity(webSecurity = true) {
        return this.mergeData({ webSecurity });
    }

    setExtensions(extensions = true) {
        return this.mergeData({ extensions });
    }

    setPlugins(plugins = true) {
        return this.mergeData({ plugins });
    }

    setJavaScript(javascript = true) {
        return this.mergeData({ javascript });
    }

    setImages(images = true) {
        return this.mergeData({ images });
    }

    setNotifications(notifications = true) {
        return this.mergeData({ notifications });
    }

    setUserAgent(userAgent) {
        return this.mergeData({ userAgent });
    }

    addPreference(key, value) {
        const preferences = { ...this._data.preferences };
        preferences[key] = value;
        return this.mergeData({ preferences });
    }

    setPreferences(preferences) {
        return this.mergeData({ preferences: { ...preferences } });
    }
}

/**
 * Session isolation configuration
 * Handles data persistence, profiles, and session sharing
 */
class IsolationConfig extends BaseConfigComponent {
    constructor() {
        super();
        this._data = {
            userDataDir: null,
            incognito: false,
            sharedSession: true,
            persistData: false,
            profileName: null,
            cookies: {},
            localStorage: {}
        };
    }

    setUserDataDir(userDataDir) {
        return this.mergeData({ userDataDir });
    }

    setIncognito(incognito = true) {
        return this.mergeData({ incognito });
    }

    setSharedSession(sharedSession = true) {
        return this.mergeData({ sharedSession });
    }

    setPersistData(persistData = true) {
        return this.mergeData({ persistData });
    }

    setProfileName(profileName) {
        return this.mergeData({ profileName });
    }

    addCookie(name, value) {
        const cookies = { ...this._data.cookies };
        cookies[name] = value;
        return this.mergeData({ cookies });
    }

    setCookies(cookies) {
        return this.mergeData({ cookies: { ...cookies } });
    }

    addLocalStorageItem(key, value) {
        const localStorage = { ...this._data.localStorage };
        localStorage[key] = value;
        return this.mergeData({ localStorage });
    }

    setLocalStorage(localStorage) {
        return this.mergeData({ localStorage: { ...localStorage } });
    }
}

/**
 * Automation features configuration
 * Controls timeouts, capture settings, and lifecycle management
 */
class AutomationConfig extends BaseConfigComponent {
    constructor() {
        super();
        this._data = {
            autoClose: false,
            timeout: 300000, // 5 minutes
            autoRestart: true,
            captureErrors: true,
            captureConsole: true,
            captureNetwork: false,
            startUrl: 'http://localhost:9000',
            preloadScripts: []
        };

        this.addValidator((data) => {
            if (data.timeout <= 0) {
                return 'Automation timeout must be positive';
            }
            return true;
        });
    }

    setAutoClose(autoClose = true) {
        return this.mergeData({ autoClose });
    }

    setTimeout(timeout) {
        return this.mergeData({ timeout });
    }

    setAutoRestart(autoRestart = true) {
        return this.mergeData({ autoRestart });
    }

    setCaptureErrors(captureErrors = true) {
        return this.mergeData({ captureErrors });
    }

    setCaptureConsole(captureConsole = true) {
        return this.mergeData({ captureConsole });
    }

    setCaptureNetwork(captureNetwork = true) {
        return this.mergeData({ captureNetwork });
    }

    setStartUrl(startUrl) {
        return this.mergeData({ startUrl });
    }

    addPreloadScript(script) {
        const preloadScripts = [...this._data.preloadScripts, script];
        return this.mergeData({ preloadScripts });
    }

    setPreloadScripts(preloadScripts) {
        return this.mergeData({ preloadScripts: [...preloadScripts] });
    }
}

/**
 * Main session configuration builder
 * Implements fluent builder pattern with method chaining
 */
class SessionConfigBuilder {
    constructor(purpose = SessionPurpose.DEVELOPMENT, aiPersona = 'system') {
        this.purpose = purpose;
        this.aiPersona = aiPersona;
        
        // Initialize configuration components
        this.windowConfig = new WindowConfig();
        this.displayConfig = new DisplayConfig();
        this.browserConfig = new BrowserConfig();
        this.isolationConfig = new IsolationConfig();
        this.automationConfig = new AutomationConfig();
    }

    // ========================================
    // BUILDER CONFIGURATION METHODS
    // ========================================

    setPurpose(purpose) {
        if (!Object.values(SessionPurpose).includes(purpose)) {
            throw new Error(`Invalid session purpose: ${purpose}`);
        }
        this.purpose = purpose;
        return this;
    }

    setAIPersona(aiPersona) {
        this.aiPersona = aiPersona;
        return this;
    }

    // Window configuration
    window() {
        return {
            setDimensions: (width, height) => {
                this.windowConfig.setDimensions(width, height);
                return this;
            },
            setPosition: (x, y) => {
                this.windowConfig.setPosition(x, y);
                return this;
            },
            setTitle: (title) => {
                this.windowConfig.setTitle(title);
                return this;
            },
            maximize: () => {
                this.windowConfig.maximize();
                return this;
            },
            minimize: () => {
                this.windowConfig.minimize();
                return this;
            },
            fullscreen: () => {
                this.windowConfig.fullscreen();
                return this;
            },
            alwaysOnTop: (enabled = true) => {
                this.windowConfig.setAlwaysOnTop(enabled);
                return this;
            },
            setOpacity: (opacity) => {
                this.windowConfig.setOpacity(opacity);
                return this;
            }
        };
    }

    // Display configuration
    display() {
        return {
            headless: () => {
                this.displayConfig.setHeadless(true);
                return this;
            },
            visible: () => {
                this.displayConfig.setVisible(true);
                return this;
            },
            kiosk: () => {
                this.displayConfig.setKiosk(true);
                return this;
            },
            setDisplay: (displayId) => {
                this.displayConfig.setDisplayId(displayId);
                return this;
            }
        };
    }

    // Browser configuration
    browser() {
        return {
            setEngine: (engine) => {
                this.browserConfig.setEngine(engine);
                return this;
            },
            debugging: (enabled = true) => {
                this.browserConfig.setDebugging(enabled);
                return this;
            },
            webSecurity: (enabled = true) => {
                this.browserConfig.setWebSecurity(enabled);
                return this;
            },
            extensions: (enabled = true) => {
                this.browserConfig.setExtensions(enabled);
                return this;
            },
            setUserAgent: (userAgent) => {
                this.browserConfig.setUserAgent(userAgent);
                return this;
            }
        };
    }

    // Isolation configuration
    isolation() {
        return {
            incognito: () => {
                this.isolationConfig.setIncognito(true);
                return this;
            },
            shared: () => {
                this.isolationConfig.setSharedSession(true);
                return this;
            },
            persistent: () => {
                this.isolationConfig.setPersistData(true);
                return this;
            },
            setProfile: (profileName) => {
                this.isolationConfig.setProfileName(profileName);
                return this;
            }
        };
    }

    // Automation configuration
    automation() {
        return {
            autoClose: () => {
                this.automationConfig.setAutoClose(true);
                return this;
            },
            setTimeout: (timeout) => {
                this.automationConfig.setTimeout(timeout);
                return this;
            },
            captureAll: () => {
                this.automationConfig.setCaptureErrors(true);
                this.automationConfig.setCaptureConsole(true);
                this.automationConfig.setCaptureNetwork(true);
                return this;
            },
            setStartUrl: (url) => {
                this.automationConfig.setStartUrl(url);
                return this;
            }
        };
    }

    // ========================================
    // PRESET CONFIGURATIONS
    // ========================================

    development() {
        this.displayConfig.setVisible(true);
        this.browserConfig.setDebugging(true);
        this.browserConfig.setWebSecurity(false);
        this.browserConfig.setExtensions(true);
        this.automationConfig.setCaptureErrors(true);
        this.automationConfig.setCaptureConsole(true);
        return this;
    }

    production() {
        this.displayConfig.setHeadless(true);
        this.browserConfig.setDebugging(false);
        this.browserConfig.setExtensions(false);
        this.browserConfig.setPlugins(false);
        this.automationConfig.setCaptureErrors(false);
        this.automationConfig.setCaptureConsole(false);
        return this;
    }

    testing() {
        this.isolationConfig.setIncognito(true);
        this.isolationConfig.setSharedSession(false);
        this.isolationConfig.setPersistData(false);
        this.automationConfig.setAutoClose(true);
        this.automationConfig.setTimeout(60000); // 1 minute
        return this;
    }

    debugging() {
        this.windowConfig.setDimensions(1920, 1080);
        this.windowConfig.setAlwaysOnTop(true);
        this.displayConfig.setVisible(true);
        this.browserConfig.setDebugging(true);
        this.browserConfig.setWebSecurity(false);
        this.automationConfig.setCaptureErrors(true);
        this.automationConfig.setCaptureConsole(true);
        this.automationConfig.setCaptureNetwork(true);
        return this;
    }

    kiosk() {
        this.windowConfig.fullscreen();
        this.displayConfig.setKiosk(true);
        this.browserConfig.setExtensions(false);
        this.browserConfig.setNotifications(false);
        this.isolationConfig.setIncognito(true);
        return this;
    }

    shared() {
        this.isolationConfig.setSharedSession(true);
        this.isolationConfig.setPersistData(false);
        this.windowConfig.setTitle(`Continuum DevTools - ${this.purpose}`);
        return this;
    }

    // ========================================
    // BUILD AND VALIDATION
    // ========================================

    /**
     * Validate all configuration components
     */
    validate() {
        const allErrors = [];
        
        const configs = [
            this.windowConfig,
            this.displayConfig,
            this.browserConfig,
            this.isolationConfig,
            this.automationConfig
        ];

        for (const config of configs) {
            const validation = config.validate();
            if (!validation.valid) {
                allErrors.push(...validation.errors);
            }
        }

        return {
            valid: allErrors.length === 0,
            errors: allErrors
        };
    }

    /**
     * Build final configuration object
     */
    build() {
        const validation = this.validate();
        if (!validation.valid) {
            throw new Error(`Configuration validation failed: ${validation.errors.join(', ')}`);
        }

        return {
            purpose: this.purpose,
            aiPersona: this.aiPersona,
            window: this.windowConfig.getData(),
            display: this.displayConfig.getData(),
            browser: this.browserConfig.getData(),
            isolation: this.isolationConfig.getData(),
            automation: this.automationConfig.getData(),
            metadata: {
                created: new Date(),
                sessionId: null,
                port: null,
                windowName: null
            }
        };
    }

    /**
     * Clone builder with current configuration
     */
    clone() {
        const cloned = new SessionConfigBuilder(this.purpose, this.aiPersona);
        
        // Deep clone configuration components
        cloned.windowConfig._data = { ...this.windowConfig._data };
        cloned.displayConfig._data = { ...this.displayConfig._data };
        cloned.browserConfig._data = { ...this.browserConfig._data };
        cloned.isolationConfig._data = { ...this.isolationConfig._data };
        cloned.automationConfig._data = { ...this.automationConfig._data };
        
        return cloned;
    }

    /**
     * Export as JSON
     */
    toJSON() {
        return this.build();
    }
}

/**
 * Factory functions for common session types
 */
class SessionConfigFactory {
    static gitVerification() {
        return new SessionConfigBuilder(SessionPurpose.GIT_VERIFICATION, 'system')
            .shared()
            .debugging()
            .window().setTitle('Continuum DevTools - Git Verification');
    }

    static aiWorkspace(persona) {
        return new SessionConfigBuilder(SessionPurpose.WORKSPACE, persona)
            .shared()
            .development()
            .window().setTitle(`Continuum DevTools - ${persona} Workspace`);
    }

    static testing(purpose = 'test') {
        return new SessionConfigBuilder(purpose, 'test')
            .testing()
            .window().setDimensions(1024, 768);
    }

    static headless(purpose = 'automation') {
        return new SessionConfigBuilder(purpose, 'automation')
            .production()
            .display().headless();
    }

    static presentation() {
        return new SessionConfigBuilder(SessionPurpose.PRESENTATION, 'display')
            .kiosk()
            .automation().autoClose();
    }
}

module.exports = {
    SessionConfigBuilder,
    SessionConfigFactory,
    WindowConfig,
    DisplayConfig,
    BrowserConfig,
    IsolationConfig,
    AutomationConfig,
    SessionPurpose,
    WindowState,
    BrowserEngine,
    ScreenshotFormat
};