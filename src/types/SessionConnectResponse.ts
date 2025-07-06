/**
 * SessionConnectResponse - Strongly typed session connection response
 * 
 * This is the contract between SessionManagerDaemon and all clients
 */

export interface SessionConnectResponse {
  version: string;
  sessionId: string;
  action: 'joined_existing' | 'created_new' | 'forked_from';
  launched: {
    browser: boolean;
    webserver: boolean;
    newLogFiles: boolean;
  };
  logs: {
    browser: string;
    server: string;
  };
  interface: string;
  screenshots: string;
  commands: {
    otherClients: string;
    stop: string;
    fork: string;
    info: string;
  };
}

export interface SessionConnectError {
  error: string;
  code?: string;
}