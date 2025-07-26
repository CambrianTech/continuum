import { CommandParams, CommandResult, type JTAGContext } from '@shared/JTAGTypes';

export class ScrollParams extends CommandParams {
  x?: number;
  y?: number;
  selector?: string; // Optional element to scroll to
  behavior?: 'auto' | 'smooth' | 'instant';

  constructor(data: Partial<ScrollParams> = {}) {
    super();
    Object.assign(this, {
      x: 0,
      y: 0,
      selector: undefined,
      behavior: 'smooth',
      ...data
    });
  }
}

export class ScrollResult extends CommandResult {
  success!: boolean;
  scrollX!: number;
  scrollY!: number;
  selector?: string;
  scrolled!: boolean;
  error?: string;
  environment!: JTAGContext['environment'];
  timestamp!: string;

  constructor(data: Partial<ScrollResult>) {
    super();
    Object.assign(this, {
      success: false,
      scrollX: 0,
      scrollY: 0,
      selector: undefined,
      scrolled: false,
      environment: 'server',
      timestamp: new Date().toISOString(),
      ...data
    });
  }
}