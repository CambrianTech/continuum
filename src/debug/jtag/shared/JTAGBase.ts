/**
 * JTAG Base - Common foundation for JTAGSystem and JTAGClient
 * 
 * Simple base class that extends JTAGModule to establish inheritance
 */

import { JTAGModule } from './JTAGModule';

export abstract class JTAGBase extends JTAGModule {
  // Just extends JTAGModule for now - keeping it minimal
}