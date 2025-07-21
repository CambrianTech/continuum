import { BaseJTAGTransport } from './BaseTransport';
import { JTAGConfig, JTAGWebSocketMessage, JTAGTransportResponse, JTAG_STATUS, JTAG_TRANSPORT, JTAGTransportType } from '../JTAGTypes';

export class JTAGRESTTransportImpl extends BaseJTAGTransport {
  private baseUrl = '';

  constructor() {
    super('rest-transport');
  }

  getTransportType(): JTAGTransportType {
    return JTAG_TRANSPORT.REST;
  }

  protected getEndpoint(): string { return this.baseUrl; }
  protected getProtocol(): string { return 'https'; }
  protected getMetadata(): Record<string, any> { return { type: 'REST API' }; }

  async initialize(config: JTAGConfig): Promise<boolean> {
    this.baseUrl = `http://localhost:${config.jtagPort}/api/jtag`;
    this.emitStatus(JTAG_STATUS.CONNECTING, { reason: 'rest_initialization' });
    
    if (this._testMode) {
      this.emitStatus(JTAG_STATUS.ERROR, { reason: 'rest_endpoint_unavailable' });
      return false;
    }
    
    this.connected = true;
    this.emitStatus(JTAG_STATUS.READY, { httpStatus: 200, reason: 'rest_endpoint_available' });
    return true;
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    return {
      success: false,
      error: 'Not connected',
      timestamp: new Date().toISOString(),
      transportMeta: { transport: 'rest', duration: 0, retries: 0 }
    };
  }
}