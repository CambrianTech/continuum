import { CommandParams, CommandResult, type JTAGContext } from '@shared/JTAGTypes';

export class TypeParams extends CommandParams {
  selector!: string;
  text!: string;
  clearFirst?: boolean;
  delay?: number;

  constructor(data: Partial<TypeParams> = {}, context: JTAGContext, sessionId: string) {
    super(context, sessionId);
    Object.assign(this, {
      selector: '',
      text: '',
      clearFirst: true,
      delay: 0,
      ...data
    });
  }
}

export class TypeResult extends CommandResult {
  success!: boolean;
  selector!: string;
  typed!: boolean;
  text!: string;
  error?: string;
  timestamp!: string;

  constructor(data: Partial<TypeResult>, context: JTAGContext, sessionId: string) {
    super(context, sessionId);
    Object.assign(this, {
      success: false,
      selector: '',
      typed: false,
      text: '',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}