import { CommandParams, CommandResult } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';

export class WaitForElementParams extends CommandParams {
  selector!: string;
  timeout?: number;
  visible?: boolean;
  interval?: number;

  constructor(data: Partial<WaitForElementParams> = {}) {
    super();
    Object.assign(this, {
      selector: 'body',
      timeout: 30000,
      visible: true,
      interval: 100,
      ...data
    });
  }
}

export class WaitForElementResult extends CommandResult {
  success!: boolean;
  selector!: string;
  found!: boolean;
  visible!: boolean;
  timeout!: number;
  waitTime!: number;
  error?: string;
  environment!: JTAGContext['environment'];
  timestamp!: string;

  constructor(data: Partial<WaitForElementResult>) {
    super();
    Object.assign(this, {
      success: false,
      selector: '',
      found: false,
      visible: false,
      timeout: 30000,
      waitTime: 0,
      environment: 'server',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}