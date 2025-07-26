import { CommandParams, CommandResult } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class GetTextParams extends CommandParams {
  selector!: string;
  trim?: boolean;
  innerText?: boolean; // vs textContent

  constructor(data: Partial<GetTextParams> = {}) {
    super();
    Object.assign(this, {
      selector: 'body',
      trim: true,
      innerText: true,
      ...data
    });
  }
}

export class GetTextResult extends CommandResult {
  success!: boolean;
  selector!: string;
  text!: string;
  found!: boolean;
  error?: string;
  environment!: JTAGContext['environment'];
  timestamp!: string;

  constructor(data: Partial<GetTextResult>) {
    super();
    Object.assign(this, {
      success: false,
      selector: '',
      text: '',
      found: false,
      environment: 'server',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}