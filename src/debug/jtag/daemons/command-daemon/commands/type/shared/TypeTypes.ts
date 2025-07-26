import { CommandParams, CommandResult, type JTAGContext } from '@shared/JTAGTypes';

export class TypeParams extends CommandParams {
  selector!: string;
  text!: string;
  clearFirst?: boolean;
  delay?: number;

  constructor(data: Partial<TypeParams> = {}) {
    super();
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
  environment!: JTAGContext['environment'];
  timestamp!: string;

  constructor(data: Partial<TypeResult>) {
    super();
    Object.assign(this, {
      success: false,
      selector: '',
      typed: false,
      text: '',
      environment: 'server',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}