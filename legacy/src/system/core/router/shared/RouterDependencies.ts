/**
 * RouterDependencies - Dependency injection interfaces for testable router architecture
 * 
 * Enables clean testing by providing mockable interfaces for all router dependencies.
 * Follows dependency inversion principle for better modularity.
 */

import type { JTAGMessage, JTAGContext } from '../../types/JTAGTypes';
import type { UUID } from '../../types/CrossPlatformUUID';
import type { JTAGTransport } from '../../../transports';
import { TRANSPORT_TYPES } from '../../../transports';
import type { MessageSubscriber } from './JTAGRouter';
import type { JTAGRouterConfig, ResolvedJTAGRouterConfig } from './JTAGRouterTypes';
import type { IMessageProcessor, ProcessingResult } from './MessageProcessor';
import type { IEventDistributor } from './EventDistributor';
import type { IPromiseCorrelator } from './PromiseCorrelator';
import type { RouterResult, EventResult, RequestResult, LocalRoutingResult } from './RouterTypes';

/**
 * Core router dependencies - all mockable for testing
 */
export interface RouterDependencies {
  readonly messageProcessor: IMessageProcessor;
  readonly eventDistributor: IEventDistributor;
  readonly promiseCorrelator: IPromiseCorrelator;
  readonly subscriberRegistry: ISubscriberRegistry;
  readonly transportManager: ITransportManager;
  readonly configResolver: IConfigResolver;
}

/**
 * Subscriber registry for endpoint matching and routing
 */
export interface ISubscriberRegistry {
  register(endpoint: string, subscriber: MessageSubscriber): void;
  unregister(endpoint: string): boolean;
  match(endpoint: string): SubscriberMatch | null;
  getEndpoints(): string[];
  hasExact(endpoint: string): boolean;
  clear(): void;
  size(): number;
}

export interface SubscriberMatch {
  readonly subscriber: MessageSubscriber;
  readonly matchedEndpoint: string;
  readonly matchType: 'exact' | 'hierarchical';
}

/**
 * Transport management interface
 */
export interface ITransportManager {
  initialize(config: ResolvedJTAGRouterConfig): Promise<void>;
  getTransport(type: TRANSPORT_TYPES): JTAGTransport | null;
  registerTransport(type: TRANSPORT_TYPES, transport: JTAGTransport): void;
  removeTransport(type: TRANSPORT_TYPES): void;
  isHealthy(): boolean;
  getStatus(): TransportManagerStatus;
  shutdown(): Promise<void>;
}

export interface TransportManagerStatus {
  readonly initialized: boolean;
  readonly transportCount: number;
  readonly healthyTransports: number;
  readonly transports: Array<{
    readonly type: string;
    readonly name: string;
    readonly connected: boolean;
    readonly health?: {
      readonly latency: number;
      readonly errorCount: number;
      readonly lastActivity: string;
    };
  }>;
}

/**
 * Configuration resolver interface
 */
export interface IConfigResolver {
  resolve(config: JTAGRouterConfig): ResolvedJTAGRouterConfig;
  validate(config: ResolvedJTAGRouterConfig): boolean;
  getDefault(): ResolvedJTAGRouterConfig;
}

/**
 * Local routing handler interface
 */
export interface ILocalRouter {
  routeToSubscriber(message: JTAGMessage): Promise<LocalRoutingResult>;
  handleRequest(message: JTAGMessage): Promise<LocalRoutingResult>;
  handleResponse(message: JTAGMessage): Promise<LocalRoutingResult>;
  handleEvent(message: JTAGMessage): Promise<LocalRoutingResult>;
}

/**
 * Factory interface for creating router dependencies
 */
export interface IRouterDependencyFactory {
  createMessageProcessor(context: JTAGContext): IMessageProcessor;
  createEventDistributor(context: JTAGContext): IEventDistributor;
  createPromiseCorrelator(config: ResolvedJTAGRouterConfig): IPromiseCorrelator;
  createSubscriberRegistry(): ISubscriberRegistry;
  createTransportManager(context: JTAGContext): ITransportManager;
  createConfigResolver(): IConfigResolver;
  createLocalRouter(dependencies: RouterDependencies): ILocalRouter;
}

/**
 * Router builder interface for dependency injection
 */
export interface IRouterBuilder {
  withContext(context: JTAGContext): IRouterBuilder;
  withConfig(config: JTAGRouterConfig): IRouterBuilder;
  withMessageProcessor(processor: IMessageProcessor): IRouterBuilder;
  withEventDistributor(distributor: IEventDistributor): IRouterBuilder;
  withPromiseCorrelator(correlator: IPromiseCorrelator): IRouterBuilder;
  withSubscriberRegistry(registry: ISubscriberRegistry): IRouterBuilder;
  withTransportManager(manager: ITransportManager): IRouterBuilder;
  build(): Promise<TestableJTAGRouter>;
}

/**
 * Testable router interface - dependency injected version
 */
export interface TestableJTAGRouter {
  // Core routing methods
  postMessage<T extends RouterResult>(message: JTAGMessage): Promise<T>;
  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void;
  getSubscriber(endpoint: string): MessageSubscriber | null;

  // Lifecycle methods
  initialize(): Promise<void>;
  shutdown(): Promise<void>;

  // Testing methods
  getDependencies(): RouterDependencies;
  getStats(): RouterStats;
  isInitialized(): boolean;
}

/**
 * Router statistics for monitoring and testing
 */
export interface RouterStats {
  readonly messageProcessor: {
    activeTokens: number;
    correlations: { reqMappings: number; resMappings: number };
  };
  readonly eventDistributor: {
    registeredTransports: number;
  };
  readonly promiseCorrelator: {
    pendingRequests: number;
    oldestPendingAge: number | null;
  };
  readonly subscriberRegistry: {
    totalSubscribers: number;
    endpoints: string[];
  };
  readonly transportManager: TransportManagerStatus;
}

/**
 * Mock factory for testing - creates test doubles for all dependencies
 */
export interface IMockRouterFactory {
  createMockMessageProcessor(): IMessageProcessor;
  createMockEventDistributor(): IEventDistributor;
  createMockPromiseCorrelator(): IPromiseCorrelator;
  createMockSubscriberRegistry(): ISubscriberRegistry;
  createMockTransportManager(): ITransportManager;
  createMockConfigResolver(): IConfigResolver;
  createMockTransport(type: TRANSPORT_TYPES): JTAGTransport;
  createMockSubscriber(endpoint: string, uuid: string): MessageSubscriber;
}

/**
 * Router test scenarios interface
 */
export interface IRouterTestScenarios {
  testBasicRequestResponse(): Promise<void>;
  testEventDistribution(): Promise<void>;
  testP2PRouting(): Promise<void>;
  testTransportFailover(): Promise<void>;
  testCorrelationTimeout(): Promise<void>;
  testDuplicateMessageHandling(): Promise<void>;
  testConcurrentRequests(): Promise<void>;
  testMalformedMessageHandling(): Promise<void>;
}