/**
 * Base DevTools Adapter
 * Abstract base class for all DevTools adapters
 */

const { EventEmitter } = require('events');

class BaseAdapter extends EventEmitter {
  constructor(options = {}) {
    super();
    this.connected = false;
    this.options = options;
    this.name = this.constructor.name;
  }

  /**
   * Check if this adapter is available/supported
   */
  async isAvailable() {
    throw new Error('isAvailable() must be implemented by subclass');
  }

  /**
   * Connect to the target
   */
  async connect() {
    throw new Error('connect() must be implemented by subclass');
  }

  /**
   * Disconnect from the target
   */
  async disconnect() {
    this.connected = false;
    this.emit('disconnected');
  }

  /**
   * Send command (optional - not all adapters support this)
   */
  async send(command, data) {
    throw new Error('send() not supported by this adapter');
  }

  /**
   * Emit standardized data event
   */
  emitData(type, data) {
    this.emit('data', {
      type,
      timestamp: new Date().toISOString(),
      adapter: this.name,
      ...data
    });
  }

  /**
   * Emit standardized error event
   */
  emitError(error, context = {}) {
    this.emit('error', {
      error,
      adapter: this.name,
      timestamp: new Date().toISOString(),
      ...context
    });
  }

  /**
   * Get adapter status
   */
  getStatus() {
    return {
      name: this.name,
      connected: this.connected,
      options: this.options
    };
  }
}

module.exports = BaseAdapter;