/**
 * Simple Router - Commands vs HTML
 */

export class DaemonRouter {
  private renderer?: any;
  private commandProcessor?: any;

  register(daemon: any): void {
    if (daemon.name === 'renderer') this.renderer = daemon;
    if (daemon.name === 'command-processor') this.commandProcessor = daemon;
  }

  route(isWebSocket: boolean): any | null {
    return isWebSocket ? this.commandProcessor : this.renderer;
  }
}