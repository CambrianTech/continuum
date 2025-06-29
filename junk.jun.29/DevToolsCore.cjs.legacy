/**
 * DevTools Core
 * Base system for pluggable browser development tools integration
 */

const { EventEmitter } = require('events');

class DevToolsCore extends EventEmitter {
  constructor(options = {}) {
    super();
    this.enabled = options.enabled !== false;
    this.adapters = new Map();
    this.activeAdapter = null;
    this.config = {
      retryAttempts: 3,
      retryDelay: 1000,
      ...options
    };
  }

  /**
   * Register a DevTools adapter
   */
  registerAdapter(name, adapter) {
    if (!adapter.connect || !adapter.disconnect) {
      throw new Error(`Adapter ${name} must implement connect() and disconnect() methods`);
    }
    
    this.adapters.set(name, adapter);
    console.log(`🔌 DevTools: Registered adapter: ${name}`);
    
    // Forward adapter events
    adapter.on('data', (data) => this.emit('data', { adapter: name, ...data }));
    adapter.on('error', (error) => this.emit('error', { adapter: name, error }));
    adapter.on('connected', () => this.emit('adapter-connected', name));
    adapter.on('disconnected', () => this.emit('adapter-disconnected', name));
  }

  /**
   * List available adapters
   */
  getAdapters() {
    return Array.from(this.adapters.keys());
  }

  /**
   * Connect using specified adapter
   */
  async connect(adapterName = 'auto') {
    if (!this.enabled) {
      console.log('🔌 DevTools: Integration disabled');
      return false;
    }

    if (adapterName === 'auto') {
      adapterName = await this.detectBestAdapter();
    }

    const adapter = this.adapters.get(adapterName);
    if (!adapter) {
      throw new Error(`Unknown adapter: ${adapterName}`);
    }

    console.log(`🔌 DevTools: Connecting with adapter: ${adapterName}`);
    
    try {
      const success = await adapter.connect();
      if (success) {
        this.activeAdapter = { name: adapterName, instance: adapter };
        this.emit('connected', adapterName);
        return true;
      }
    } catch (error) {
      console.error(`🔌 DevTools: Failed to connect with ${adapterName}:`, error);
      this.emit('error', { adapter: adapterName, error });
    }
    
    return false;
  }

  /**
   * Auto-detect best available adapter
   */
  async detectBestAdapter() {
    const preferredOrder = ['chrome-devtools', 'console-injection', 'network-proxy'];
    
    for (const name of preferredOrder) {
      const adapter = this.adapters.get(name);
      if (adapter && await adapter.isAvailable()) {
        return name;
      }
    }
    
    return this.adapters.keys().next().value || null;
  }

  /**
   * Disconnect current adapter
   */
  async disconnect() {
    if (this.activeAdapter) {
      await this.activeAdapter.instance.disconnect();
      this.activeAdapter = null;
      this.emit('disconnected');
    }
  }

  /**
   * Send command through active adapter
   */
  async send(command, data = {}) {
    if (!this.activeAdapter) {
      throw new Error('No active adapter');
    }
    
    if (this.activeAdapter.instance.send) {
      return await this.activeAdapter.instance.send(command, data);
    }
    
    throw new Error(`Adapter ${this.activeAdapter.name} does not support sending commands`);
  }

  /**
   * Get status of DevTools integration
   */
  getStatus() {
    return {
      enabled: this.enabled,
      connected: !!this.activeAdapter,
      activeAdapter: this.activeAdapter?.name || null,
      availableAdapters: this.getAdapters()
    };
  }
}

module.exports = DevToolsCore;