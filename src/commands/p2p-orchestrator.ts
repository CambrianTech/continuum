#!/usr/bin/env npx tsx
/**
 * P2P Decentralized Command Orchestrator
 * BitTorrent-style distributed execution without central authority
 * Each node is a peer that discovers, routes, and executes commands
 */

import * as crypto from 'crypto';
import { EventEmitter } from 'events';

interface PeerNode {
  id: string;
  publicKey: string;
  capabilities: string[];
  location: string;
  load: number;
  reputation: number; // Trust score based on successful executions
  connections: Set<string>; // Connected peer IDs
  lastSeen: Date;
}

interface CommandRequest {
  id: string;
  command: string;
  params: any;
  requiredCapabilities: string[];
  priority: number;
  ttl: number; // Time to live for request propagation
  signature: string; // Cryptographic signature
  originPeer: string;
  routingPath: string[]; // Track routing path
}

interface CommandResult {
  requestId: string;
  executorPeer: string;
  success: boolean;
  data?: any;
  error?: string;
  proof: string; // Cryptographic proof of execution
  replicas: string[]; // Peers that have result copies
}

interface DHTEntry {
  capability: string;
  peers: Array<{
    id: string;
    load: number;
    reputation: number;
    distance: number; // Network distance
  }>;
  lastUpdated: Date;
}

class P2PCommandOrchestrator extends EventEmitter {
  private nodeId: string;
  private privateKey: string;
  private publicKey: string;
  private peers = new Map<string, PeerNode>();
  private dht = new Map<string, DHTEntry>(); // Distributed Hash Table
  private commandQueue: CommandRequest[] = [];
  private resultCache = new Map<string, CommandResult>();
  private myCapabilities: string[];
  private maxConnections = 8; // BitTorrent-style connection limit
  
  constructor(capabilities: string[], location: string = 'unknown') {
    super();
    
    // Generate cryptographic identity
    const keyPair = this.generateKeyPair();
    this.nodeId = this.generateNodeId(keyPair.publicKey);
    this.privateKey = keyPair.privateKey;
    this.publicKey = keyPair.publicKey;
    this.myCapabilities = capabilities;
    
    console.log(`🆔 P2P Node initialized: ${this.nodeId.substring(0, 8)}...`);
    console.log(`📍 Location: ${location}`);
    console.log(`⚡ Capabilities: ${capabilities.join(', ')}`);
    
    this.setupPeerDiscovery();
    this.setupCommandProcessing();
    this.setupDHTMaintenance();
  }

  /**
   * Connect to the P2P network via bootstrap peers
   */
  async connectToNetwork(bootstrapPeers: string[] = []): Promise<void> {
    console.log(`🌐 Connecting to P2P network...`);
    
    // In real implementation, this would use WebRTC, libp2p, or similar
    // For demo, we'll simulate peer discovery
    
    for (const bootstrapPeer of bootstrapPeers) {
      await this.connectToPeer(bootstrapPeer);
    }
    
    // Start DHT bootstrapping
    await this.bootstrapDHT();
    
    console.log(`✅ Connected to ${this.peers.size} peers`);
  }

  /**
   * Execute command via P2P network
   */
  async executeP2P(command: string, params: any, priority: number = 5): Promise<string> {
    const requestId = crypto.randomUUID();
    
    // Determine required capabilities
    const requiredCapabilities = this.getCommandCapabilities(command);
    
    const request: CommandRequest = {
      id: requestId,
      command,
      params,
      requiredCapabilities,
      priority,
      ttl: 7, // Max 7 hops (BitTorrent-style)
      signature: this.signRequest(requestId, command, params),
      originPeer: this.nodeId,
      routingPath: [this.nodeId]
    };
    
    console.log(`🚀 P2P execute: ${command} (${requestId.substring(0, 8)}...)`);
    console.log(`  Required capabilities: ${requiredCapabilities.join(', ')}`);
    
    // Check if we can execute locally
    if (this.canExecuteLocally(requiredCapabilities)) {
      console.log(`  💻 Executing locally`);
      return this.executeLocally(request);
    }
    
    // Find capable peers via DHT
    const capablePeers = await this.findCapablePeers(requiredCapabilities);
    
    if (capablePeers.length === 0) {
      // Flood the network (BitTorrent-style peer discovery)
      console.log(`  🌊 Flooding network for capabilities...`);
      await this.floodRequest(request);
    } else {
      // Route to best peer
      const bestPeer = this.selectBestPeer(capablePeers);
      console.log(`  📡 Routing to: ${bestPeer.substring(0, 8)}...`);
      await this.routeToPeer(request, bestPeer);
    }
    
    return requestId;
  }

  /**
   * Generate cryptographic key pair
   */
  private generateKeyPair(): { privateKey: string; publicKey: string } {
    // Simplified - in production use proper crypto libraries
    const privateKey = crypto.randomBytes(32).toString('hex');
    const publicKey = crypto.createHash('sha256').update(privateKey).digest('hex');
    return { privateKey, publicKey };
  }

  /**
   * Generate node ID from public key
   */
  private generateNodeId(publicKey: string): string {
    return crypto.createHash('sha1').update(publicKey).digest('hex');
  }

  /**
   * Sign command request
   */
  private signRequest(requestId: string, command: string, params: any): string {
    const data = `${requestId}:${command}:${JSON.stringify(params)}`;
    return crypto.createHmac('sha256', this.privateKey).update(data).digest('hex');
  }

  /**
   * Setup peer discovery mechanism
   */
  private setupPeerDiscovery(): void {
    // Periodic peer discovery (BitTorrent-style)
    setInterval(() => {
      this.discoverNewPeers();
    }, 30000); // Every 30 seconds
    
    // Maintain connection health
    setInterval(() => {
      this.maintainConnections();
    }, 10000); // Every 10 seconds
  }

  /**
   * Setup command processing
   */
  private setupCommandProcessing(): void {
    // Process incoming commands
    this.on('command_request', (request: CommandRequest) => {
      this.handleIncomingRequest(request);
    });
    
    // Process command results
    this.on('command_result', (result: CommandResult) => {
      this.handleCommandResult(result);
    });
  }

  /**
   * Setup DHT maintenance
   */
  private setupDHTMaintenance(): void {
    // Update DHT with our capabilities
    setInterval(() => {
      this.announceDHTCapabilities();
    }, 60000); // Every minute
    
    // Clean up stale DHT entries
    setInterval(() => {
      this.cleanupDHT();
    }, 300000); // Every 5 minutes
  }

  /**
   * Connect to a specific peer
   */
  private async connectToPeer(peerId: string): Promise<void> {
    // Simulate peer connection
    const simulatedPeer: PeerNode = {
      id: peerId,
      publicKey: crypto.randomBytes(32).toString('hex'),
      capabilities: this.generateRandomCapabilities(),
      location: this.generateRandomLocation(),
      load: Math.random() * 100,
      reputation: Math.random() * 100,
      connections: new Set([this.nodeId]),
      lastSeen: new Date()
    };
    
    this.peers.set(peerId, simulatedPeer);
    console.log(`🤝 Connected to peer: ${peerId.substring(0, 8)}... (${simulatedPeer.location})`);
  }

  /**
   * Bootstrap DHT with initial entries
   */
  private async bootstrapDHT(): Promise<void> {
    console.log(`🗂️ Bootstrapping DHT...`);
    
    // Announce our capabilities to the network
    await this.announceDHTCapabilities();
    
    // Request capabilities from connected peers
    for (const [peerId, peer] of this.peers) {
      this.requestPeerCapabilities(peerId);
    }
  }

  /**
   * Get command capabilities mapping
   */
  private getCommandCapabilities(command: string): string[] {
    const capabilityMap = new Map([
      ['screenshot', ['screenshot', 'browser']],
      ['emotion', ['emotion-expression', 'visual-feedback']],
      ['validation', ['validation', 'code-analysis']],
      ['cursor', ['cursor-control', 'mouse-control']],
      ['type', ['keyboard-control', 'text-input']],
      ['share', ['sharing', 'communication']],
      ['issues', ['github-api', 'ai-collaboration']]
    ]);
    
    return capabilityMap.get(command) || [command];
  }

  /**
   * Check if we can execute command locally
   */
  private canExecuteLocally(requiredCapabilities: string[]): boolean {
    return requiredCapabilities.every(cap =>
      this.myCapabilities.some(myCap => 
        myCap.includes(cap) || cap.includes(myCap)
      )
    );
  }

  /**
   * Execute command locally
   */
  private async executeLocally(request: CommandRequest): Promise<string> {
    console.log(`  💻 Executing ${request.command} locally...`);
    
    // Simulate local execution
    const executionTime = Math.random() * 1000;
    
    return new Promise((resolve) => {
      setTimeout(() => {
        const result: CommandResult = {
          requestId: request.id,
          executorPeer: this.nodeId,
          success: Math.random() > 0.1, // 90% success rate
          data: this.generateMockResult(request.command),
          proof: this.generateExecutionProof(request),
          replicas: [this.nodeId]
        };
        
        if (!result.success) {
          result.error = 'Local execution failed';
        }
        
        // Cache result locally
        this.resultCache.set(request.id, result);
        
        // Replicate result to peers for fault tolerance
        this.replicateResult(result);
        
        console.log(`  ✅ Local execution complete: ${result.success ? 'SUCCESS' : 'FAILED'}`);
        resolve(request.id);
      }, executionTime);
    });
  }

  /**
   * Find capable peers via DHT
   */
  private async findCapablePeers(requiredCapabilities: string[]): Promise<string[]> {
    const capablePeers: string[] = [];
    
    for (const capability of requiredCapabilities) {
      const dhtEntry = this.dht.get(capability);
      if (dhtEntry) {
        // Sort by reputation and load
        const sortedPeers = dhtEntry.peers
          .sort((a, b) => (b.reputation - a.reputation) + (a.load - b.load))
          .slice(0, 3); // Top 3 peers
        
        capablePeers.push(...sortedPeers.map(p => p.id));
      }
    }
    
    return [...new Set(capablePeers)]; // Remove duplicates
  }

  /**
   * Select best peer based on multiple factors
   */
  private selectBestPeer(capablePeers: string[]): string {
    let bestPeer = capablePeers[0];
    let bestScore = -1;
    
    for (const peerId of capablePeers) {
      const peer = this.peers.get(peerId);
      if (!peer) continue;
      
      // Calculate score: reputation - load + connection quality
      const score = peer.reputation - peer.load + (peer.connections.size * 10);
      
      if (score > bestScore) {
        bestScore = score;
        bestPeer = peerId;
      }
    }
    
    return bestPeer;
  }

  /**
   * Route command to specific peer
   */
  private async routeToPeer(request: CommandRequest, peerId: string): Promise<void> {
    request.routingPath.push(peerId);
    
    // In real implementation, send via WebRTC/WebSocket
    console.log(`  📡 Routing to peer: ${peerId.substring(0, 8)}...`);
    
    // Simulate network delay
    setTimeout(() => {
      this.emit('command_request', request);
    }, Math.random() * 100);
  }

  /**
   * Flood request to all connected peers (BitTorrent-style)
   */
  private async floodRequest(request: CommandRequest): Promise<void> {
    if (request.ttl <= 0) {
      console.log(`  ⏰ Request TTL expired: ${request.id.substring(0, 8)}...`);
      return;
    }
    
    request.ttl--;
    
    for (const [peerId, peer] of this.peers) {
      if (!request.routingPath.includes(peerId)) {
        await this.routeToPeer({ ...request }, peerId);
      }
    }
  }

  /**
   * Handle incoming command request
   */
  private async handleIncomingRequest(request: CommandRequest): Promise<void> {
    console.log(`📥 Incoming request: ${request.command} from ${request.originPeer.substring(0, 8)}...`);
    
    // Check if we already processed this request
    if (this.resultCache.has(request.id)) {
      console.log(`  📋 Already processed, returning cached result`);
      return;
    }
    
    // Check if we can execute
    if (this.canExecuteLocally(request.requiredCapabilities)) {
      await this.executeLocally(request);
    } else {
      // Forward to other peers
      console.log(`  📡 Forwarding to other peers...`);
      await this.floodRequest(request);
    }
  }

  /**
   * Handle command result
   */
  private handleCommandResult(result: CommandResult): void {
    console.log(`📨 Result received: ${result.requestId.substring(0, 8)}... from ${result.executorPeer.substring(0, 8)}...`);
    
    // Cache result
    this.resultCache.set(result.requestId, result);
    
    // Update peer reputation
    const executorPeer = this.peers.get(result.executorPeer);
    if (executorPeer) {
      executorPeer.reputation += result.success ? 1 : -2;
      executorPeer.reputation = Math.max(0, Math.min(100, executorPeer.reputation));
    }
  }

  /**
   * Announce our capabilities to DHT
   */
  private async announceDHTCapabilities(): Promise<void> {
    for (const capability of this.myCapabilities) {
      const entry: DHTEntry = this.dht.get(capability) || {
        capability,
        peers: [],
        lastUpdated: new Date()
      };
      
      // Remove our old entry
      entry.peers = entry.peers.filter(p => p.id !== this.nodeId);
      
      // Add our current entry
      entry.peers.push({
        id: this.nodeId,
        load: this.getCurrentLoad(),
        reputation: 100, // Self-reported
        distance: 0
      });
      
      entry.lastUpdated = new Date();
      this.dht.set(capability, entry);
    }
  }

  /**
   * Request capabilities from peer
   */
  private requestPeerCapabilities(peerId: string): void {
    // In real implementation, send DHT query
    // For demo, simulate receiving capabilities
    const peer = this.peers.get(peerId);
    if (!peer) return;
    
    for (const capability of peer.capabilities) {
      const entry: DHTEntry = this.dht.get(capability) || {
        capability,
        peers: [],
        lastUpdated: new Date()
      };
      
      entry.peers.push({
        id: peerId,
        load: peer.load,
        reputation: peer.reputation,
        distance: 1 // One hop away
      });
      
      this.dht.set(capability, entry);
    }
  }

  /**
   * Clean up stale DHT entries
   */
  private cleanupDHT(): void {
    const now = new Date();
    const maxAge = 10 * 60 * 1000; // 10 minutes
    
    for (const [capability, entry] of this.dht) {
      if (now.getTime() - entry.lastUpdated.getTime() > maxAge) {
        this.dht.delete(capability);
      } else {
        // Remove stale peer entries
        entry.peers = entry.peers.filter(peer => {
          const peerNode = this.peers.get(peer.id);
          return peerNode && (now.getTime() - peerNode.lastSeen.getTime() < maxAge);
        });
      }
    }
  }

  /**
   * Generate execution proof
   */
  private generateExecutionProof(request: CommandRequest): string {
    const data = `${request.id}:${this.nodeId}:${Date.now()}`;
    return crypto.createHmac('sha256', this.privateKey).update(data).digest('hex');
  }

  /**
   * Replicate result to peers for fault tolerance
   */
  private replicateResult(result: CommandResult): void {
    const replicationFactor = 3; // Store on 3 peers
    const targetPeers = Array.from(this.peers.keys()).slice(0, replicationFactor);
    
    for (const peerId of targetPeers) {
      // In real implementation, send result to peer
      result.replicas.push(peerId);
    }
    
    console.log(`  🔄 Result replicated to ${targetPeers.length} peers`);
  }

  /**
   * Generate mock result for demo
   */
  private generateMockResult(command: string): any {
    const results = {
      screenshot: { url: 'screenshot_' + Date.now() + '.png', size: '1920x1080' },
      emotion: { emotion: 'joy', intensity: 'high' },
      validation: { passed: true, score: 95 },
      cursor: { position: { x: 100, y: 200 } },
      type: { text: 'hello world', duration: 500 }
    };
    
    return results[command as keyof typeof results] || { status: 'completed' };
  }

  /**
   * Get current node load
   */
  private getCurrentLoad(): number {
    return Math.random() * 100; // Simplified
  }

  /**
   * Generate random capabilities for demo peers
   */
  private generateRandomCapabilities(): string[] {
    const allCapabilities = [
      'screenshot', 'browser', 'emotion-expression', 'visual-feedback',
      'validation', 'code-analysis', 'cursor-control', 'mouse-control',
      'keyboard-control', 'text-input', 'sharing', 'communication',
      'github-api', 'ai-collaboration'
    ];
    
    const count = Math.floor(Math.random() * 4) + 2; // 2-5 capabilities
    const shuffled = allCapabilities.sort(() => 0.5 - Math.random());
    return shuffled.slice(0, count);
  }

  /**
   * Generate random location for demo
   */
  private generateRandomLocation(): string {
    const locations = [
      'San Francisco, CA', 'London, UK', 'Tokyo, Japan', 'Sydney, Australia',
      'New York, NY', 'Berlin, Germany', 'Singapore', 'Toronto, Canada'
    ];
    return locations[Math.floor(Math.random() * locations.length)];
  }

  /**
   * Discover new peers
   */
  private discoverNewPeers(): void {
    // BitTorrent-style peer exchange
    if (this.peers.size < this.maxConnections) {
      // In real implementation, use DHT peer discovery
      const newPeerId = crypto.randomUUID();
      this.connectToPeer(newPeerId);
    }
  }

  /**
   * Maintain connection health
   */
  private maintainConnections(): void {
    const now = new Date();
    const timeout = 60000; // 1 minute
    
    for (const [peerId, peer] of this.peers) {
      if (now.getTime() - peer.lastSeen.getTime() > timeout) {
        console.log(`💀 Peer disconnected: ${peerId.substring(0, 8)}...`);
        this.peers.delete(peerId);
      }
    }
  }

  /**
   * Get network status
   */
  getNetworkStatus(): any {
    const dhtStats = {
      totalEntries: this.dht.size,
      capabilities: Array.from(this.dht.keys()),
      totalPeers: new Set(Array.from(this.dht.values()).flatMap(entry => entry.peers.map(p => p.id))).size
    };
    
    return {
      nodeId: this.nodeId.substring(0, 16) + '...',
      capabilities: this.myCapabilities,
      connectedPeers: this.peers.size,
      dht: dhtStats,
      resultCache: this.resultCache.size,
      averagePeerReputation: Array.from(this.peers.values()).reduce((sum, p) => sum + p.reputation, 0) / Math.max(this.peers.size, 1)
    };
  }
}

// Demo: P2P Decentralized Orchestration
async function demonstrateP2POrchestration() {
  console.log('🌊 P2P DECENTRALIZED COMMAND ORCHESTRATION');
  console.log('=========================================\n');
  
  // Create multiple P2P nodes with different capabilities
  const nodes = [
    new P2PCommandOrchestrator(['screenshot', 'browser'], 'San Francisco'),
    new P2PCommandOrchestrator(['emotion-expression', 'visual-feedback'], 'London'),
    new P2PCommandOrchestrator(['validation', 'code-analysis'], 'Tokyo'),
    new P2PCommandOrchestrator(['cursor-control', 'mouse-control'], 'New York'),
    new P2PCommandOrchestrator(['sharing', 'communication', 'github-api'], 'Berlin')
  ];
  
  // Connect nodes to form P2P network
  console.log('🔗 Forming P2P network...\n');
  
  for (let i = 0; i < nodes.length; i++) {
    const bootstrapPeers = nodes
      .filter((_, idx) => idx !== i)
      .slice(0, 2) // Connect to first 2 other nodes
      .map(node => node['nodeId']);
    
    await nodes[i].connectToNetwork(bootstrapPeers);
  }
  
  console.log('\n🚀 Executing P2P commands...\n');
  
  // Execute commands across the decentralized network
  const primaryNode = nodes[0];
  
  const requests = [
    primaryNode.executeP2P('screenshot', { url: 'https://example.com' }, 8),
    primaryNode.executeP2P('emotion', { feeling: 'excitement' }, 6),
    primaryNode.executeP2P('validation', { code: 'console.log("hello")' }, 4),
    primaryNode.executeP2P('cursor', { x: 100, y: 200 }, 7)
  ];
  
  const requestIds = await Promise.all(requests);
  
  setTimeout(() => {
    console.log('\n📊 P2P NETWORK STATUS:');
    for (let i = 0; i < nodes.length; i++) {
      console.log(`\nNode ${i + 1}:`);
      console.log(JSON.stringify(nodes[i].getNetworkStatus(), null, 2));
    }
    
    console.log('\n✨ P2P Decentralized orchestration complete!');
    console.log('🌊 BitTorrent-style peer discovery and routing');
    console.log('🔒 Cryptographic signatures and execution proofs');
    console.log('🔄 Automatic result replication for fault tolerance');
    console.log('🌍 No central authority - fully decentralized');
    console.log('⚡ DHT-based capability discovery and load balancing');
  }, 2000);
}

// Run demonstration
if (import.meta.url === `file://${process.argv[1]}`) {
  demonstrateP2POrchestration().catch(console.error);
}

export { P2PCommandOrchestrator };