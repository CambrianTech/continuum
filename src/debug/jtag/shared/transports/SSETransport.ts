import { BaseJTAGTransport } from './BaseTransport';
import { JTAGConfig, JTAGWebSocketMessage, JTAGTransportResponse, JTAG_STATUS, JTAG_TRANSPORT, JTAGTransportType } from '../JTAGTypes';

export class JTAGSSETransportImpl extends BaseJTAGTransport {
  constructor() {
    super('sse-transport');
  }

  getTransportType(): JTAGTransportType {
    return JTAG_TRANSPORT.SSE;
  }

  protected getEndpoint(): string { return 'http://localhost:9001/events'; }
  protected getProtocol(): string { return 'sse'; }
  protected getMetadata(): Record<string, any> { return { type: 'Server-Sent Events' }; }

  async initialize(config: JTAGConfig): Promise<boolean> {
    this.emitStatus(JTAG_STATUS.CONNECTING, { reason: 'sse_initialization' });
    
    if (this._testMode) {
      this.emitStatus(JTAG_STATUS.ERROR, { reason: 'sse_endpoint_unavailable' });
      return false;
    }
    
    this.connected = true;
    this.emitStatus(JTAG_STATUS.READY, { reason: 'sse_stream_established' });
    return true;
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    return {
      success: false,
      error: 'Not connected',
      timestamp: new Date().toISOString(),
      transportMeta: { transport: 'sse', duration: 0, retries: 0 }
    };
  }
}