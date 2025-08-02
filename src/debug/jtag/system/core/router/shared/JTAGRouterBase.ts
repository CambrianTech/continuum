/**
 * JTAGRouterBase - Base Router with Abstract Transport Factory
 * 
 * Minimal base class that only abstracts transport factory selection.
 * All routing logic stays in concrete implementations.
 */

import { JTAGModule } from '../../shared/JTAGModule';
import type { JTAGContext } from '../../types/JTAGTypes';
import type { ITransportFactory } from '../../../transports';
import type { JTAGRouterConfig } from './JTAGRouterTypes';

export abstract class JTAGRouterBase extends JTAGModule {
  
  constructor(name: string, context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(name, context);
  }

  /**
   * Get environment-specific transport factory
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;
}