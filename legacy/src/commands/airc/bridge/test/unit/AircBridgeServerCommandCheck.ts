#!/usr/bin/env tsx

import { AircBridgeServerCommand } from '../../server/AircBridgeServerCommand';
import { generateUUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGRouter } from '../../../../../system/core/router/shared/JTAGRouter';
import { SYSTEM_SCOPES } from '../../../../../system/core/types/SystemScopes';
import type { JTAGConfig, JTAGTestConfiguration } from '../../../../../system/shared/SecureConfigTypes';

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
  console.log(`ok - ${message}`);
}

async function assertRejects(promise: Promise<unknown>, message: string): Promise<void> {
  const rejected = await promise.then(
    () => false,
    () => true,
  );
  assert(rejected, message);
}

const testConfiguration: JTAGTestConfiguration = {
  server: { port: 9001, host: 'localhost', protocol: 'ws' },
  client: { ui_port: 9000, host: 'localhost', protocol: 'http' },
  test_settings: {
    timeout_ms: 1000,
    retry_attempts: 0,
    screenshot_on_failure: false,
    cleanup_after_test: true,
  },
  environment: {
    test_mode: true,
    verbose_logging: false,
    isolated_sessions: true,
  },
};

const config: JTAGConfig = {
  instance: {
    name: 'airc-bridge-test',
    description: 'AIRC bridge unit test context',
    ports: { http_server: 9000, websocket_server: 9001 },
    paths: { directory: '.', html_file: 'index.html', build_output: 'dist' },
    capabilities: {},
  },
  server: {
    server: {
      port: 9001,
      host: 'localhost',
      protocol: 'ws',
      bind_interface: '127.0.0.1',
      max_connections: 1,
      enable_cors: false,
    },
    paths: {
      logs: '.continuum/logs',
      screenshots: '.continuum/screenshots',
      data_directory: '.continuum/data',
      pid_file: '.continuum/test.pid',
    },
    security: {
      enable_authentication: false,
      session_timeout_ms: 1000,
      rate_limiting: { enabled: false, requests_per_minute: 0 },
    },
    environment: { log_level: 'error', debug_mode: false },
    storage: {
      strategy: 'memory',
      backend: 'memory',
      paths: { data: '.continuum/data', backups: '.continuum/backups' },
    },
  },
  client: {
    client: {
      ui_port: 9000,
      host: 'localhost',
      protocol: 'http',
      auto_connect: false,
      reconnect_attempts: 0,
    },
    browser: {
      headless: true,
      devtools: false,
      width: 800,
      height: 600,
      user_agent: 'airc-bridge-test',
    },
    ui: {
      theme: 'dark',
      enable_animations: false,
      show_debug_panel: false,
    },
  },
  test: testConfiguration,
};

const commander: ICommandDaemon = {
  subpath: 'commands',
  get router(): JTAGRouter {
    throw new Error('router is not used by AircBridgeServerCommand unit checks');
  },
  commands: new Map(),
};

const context: JTAGContext = {
  uuid: generateUUID(),
  environment: 'server',
  config,
  getConfig: () => ({ type: 'test', config: testConfiguration }),
};

async function run(): Promise<void> {
  const command = new AircBridgeServerCommand(context, 'airc/bridge', commander);
  const sessionId = generateUUID();

  const result = await command.execute({
    context,
    sessionId,
    userId: SYSTEM_SCOPES.ANONYMOUS_USER,
    message: '!continuum ping',
    senderNick: 'mac-codex',
    channel: 'general',
    dryRun: true,
  });

  assert(result.success === true, 'dry-run command succeeds');
  assert(result.handled === false, 'dry-run does not execute bridge action');
  assert(result.parsed.action === 'ping', 'dry-run returns parsed directive');
  assert(result.responseText === 'dry-run: ping -> general', 'dry-run response is deterministic');

  await assertRejects(
    command.execute({
      context,
      sessionId,
      userId: SYSTEM_SCOPES.ANONYMOUS_USER,
      message: '',
    }),
    'missing message rejects through command boundary',
  );

  console.log('AircBridge server command checks passed');
}

void run();
