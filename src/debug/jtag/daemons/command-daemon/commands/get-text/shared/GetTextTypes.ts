import { CommandParams, CommandResult } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class GetTextParams extends CommandParams {
  selector!: string;
  trim?: boolean;
  innerText?: boolean; // vs textContent

  constructor(data: Partial<GetTextParams> = {}, context: JTAGContext, sessionId: string) {
    super(context, sessionId);
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
  timestamp!: string;

  constructor(data: Partial<GetTextResult>, context: JTAGContext, sessionId: string) {
    super(context, sessionId);
    Object.assign(this, {
      success: false,
      selector: '',
      text: '',
      found: false,
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}