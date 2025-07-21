import { BaseJTAGTransport } from './BaseTransport';
import { JTAGConfig, JTAGWebSocketMessage, JTAGTransportResponse, JTAG_STATUS, JTAG_TRANSPORT, JTAGTransportType } from '../JTAGTypes';

export class JTAGHTTPTransportImpl extends BaseJTAGTransport {
  private baseUrl = '';

  constructor() {
    super('http-transport');
  }

  getTransportType(): JTAGTransportType {
    return JTAG_TRANSPORT.HTTP;
  }

  protected getEndpoint(): string { return this.baseUrl; }
  protected getProtocol(): string { return 'http'; }
  protected getMetadata(): Record<string, any> { return { type: 'HTTP Request-Response' }; }

  async initialize(config: JTAGConfig): Promise<boolean> {
    this.baseUrl = `http://localhost:${config.jtagPort}`;
    this.emitStatus(JTAG_STATUS.CONNECTING, { reason: 'http_initialization' });
    
    if (this._testMode) {
      this.emitStatus(JTAG_STATUS.ERROR, { reason: 'http_endpoint_unavailable' });
      return false;
    }
    
    this.connected = true;
    this.emitStatus(JTAG_STATUS.READY, { httpStatus: 200, reason: 'http_endpoint_available' });
    return true;
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    return {
      success: false,
      error: 'Not connected',
      timestamp: new Date().toISOString(),
      transportMeta: { transport: 'http', duration: 0, retries: 0 }
    };
  }
}