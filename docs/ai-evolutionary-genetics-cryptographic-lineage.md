# Cryptographic Lineage Tracking for AI Evolution

## The Problem

In an evolutionary AI ecosystem, **trust and provenance are critical**:
- How do you know a persona wasn't tampered with?
- How do you verify lineage claims?
- How do you prevent malicious forks from impersonating legitimate ones?
- How do you ensure training data integrity?

**Solution:** Apply cryptographic principles used in blockchain and git to create verifiable, tamper-proof lineage.

## Core Cryptographic Primitives

### 1. Content-Addressable Storage (Like Git)

```typescript
// Every persona version gets a unique hash of its content
function hashPersona(persona: PersonaPackage): string {
  const content = {
    code: persona.code,
    loraAdapters: persona.loraAdapters,
    trainingData: persona.trainingData,
    metadata: persona.metadata,
  };

  // SHA-256 hash of complete content
  return sha256(JSON.stringify(content));
}

// Hash becomes the immutable identifier
const personaHash = "e3b0c442..."; // Unique fingerprint
```

**Benefits:**
- Change ANY byte → Hash changes
- Same content → Same hash (reproducible)
- Can't fake provenance (hash proves content)

### 2. Merkle Trees for Efficient Verification

```typescript
// Build tree of hashes for efficient partial verification
interface MerkleTree {
  root: string;           // Top-level hash
  leaves: {
    code: string;         // Hash of code
    adapters: string;     // Hash of LoRA weights
    training: string;     // Hash of training data
    metadata: string;     // Hash of metadata
  };
}

// Verify specific component without downloading everything
function verifyComponent(
  tree: MerkleTree,
  component: 'code' | 'adapters' | 'training' | 'metadata',
  actualHash: string
): boolean {
  const expectedHash = tree.leaves[component];
  return actualHash === expectedHash;
}
```

**Benefits:**
- Download only what you need
- Verify pieces independently
- Efficient for large packages

### 3. Digital Signatures for Authorship

```typescript
// Sign packages with private key
interface SignedPersona {
  personaHash: string;      // What's being signed
  signature: string;        // Cryptographic signature
  publicKey: string;        // Signer's public key
  timestamp: number;        // When signed
}

// Create signature
function signPersona(
  personaHash: string,
  privateKey: PrivateKey
): string {
  return ed25519.sign(personaHash, privateKey);
}

// Verify signature
function verifySignature(signed: SignedPersona): boolean {
  return ed25519.verify(
    signed.signature,
    signed.personaHash,
    signed.publicKey
  );
}
```

**Benefits:**
- Prove who created a persona
- Can't forge signatures without private key
- Non-repudiation (can't deny authorship)

### 4. Chain of Custody (Like Blockchain)

```typescript
// Each version points to parent with cryptographic link
interface PersonaVersion {
  hash: string;              // This version's hash
  parentHash: string;        // Parent version's hash
  signature: string;         // Creator's signature
  timestamp: number;         // When created

  // Changes from parent
  diff: {
    codeChanges: Diff;
    adapterChanges: Diff;
    trainingAdditions: TrainingData;
  };
}

// Chain forms cryptographically linked history
const chain = [
  { hash: "aaa111", parentHash: null, ... },         // Genesis
  { hash: "bbb222", parentHash: "aaa111", ... },     // Fork 1
  { hash: "ccc333", parentHash: "bbb222", ... },     // Evolution
];
```

**Benefits:**
- Tamper-proof history
- Trace lineage backward to origin
- Detect altered histories

## Cryptographic Lineage Schema

### Complete Lineage Record

```typescript
interface CryptographicLineage {
  // Identity (Content-Addressed)
  personaHash: string;          // SHA-256 of complete content
  version: string;              // Semantic version (1.2.3)

  // Authorship (Digitally Signed)
  creator: {
    publicKey: string;          // Ed25519 public key
    signature: string;          // Signature of personaHash
    identity: string;           // Human-readable name
    timestamp: number;          // Creation time
  };

  // Lineage (Merkle Chain)
  ancestry: {
    parentHash: string;         // Parent version hash
    forkPoint: string;          // Where it diverged
    lineageDepth: number;       // Generations from origin
    merkleRoot: string;         // Root of ancestry tree
  };

  // Content Verification (Merkle Tree)
  contentTree: {
    root: string;               // Overall content hash
    code: string;               // Code hash
    adapters: string;           // LoRA weights hash
    training: string;           // Training data hash
    metadata: string;           // Metadata hash
  };

  // Training Provenance
  training: {
    datasetHashes: string[];    // Hash of each dataset
    fineTuningJobs: {
      jobId: string;
      inputHash: string;        // Hash of input data
      outputHash: string;       // Hash of resulting weights
      hyperparameters: string;  // Hash of config
      timestamp: number;
    }[];
  };

  // Attestations (Multi-sig)
  attestations: {
    auditor: string;            // Who audited
    auditHash: string;          // Hash of audit report
    signature: string;          // Auditor's signature
    passed: boolean;            // Did it pass?
    timestamp: number;
  }[];

  // Reputation (Verifiable)
  reputation: {
    adoptionCount: number;
    forkCount: number;
    ratingHash: string;         // Hash of all ratings
    fitnessScore: number;
    ratingSignatures: string[]; // Signatures from raters
  };
}
```

## Verification Workflows

### 1. Full Package Verification

```typescript
async function verifyPersonaPackage(
  package: PersonaPackage,
  lineage: CryptographicLineage
): Promise<VerificationResult> {

  // Step 1: Verify content hash
  const computedHash = hashPersona(package);
  if (computedHash !== lineage.personaHash) {
    return { valid: false, reason: "Content tampered" };
  }

  // Step 2: Verify creator signature
  const signatureValid = ed25519.verify(
    lineage.creator.signature,
    lineage.personaHash,
    lineage.creator.publicKey
  );
  if (!signatureValid) {
    return { valid: false, reason: "Invalid signature" };
  }

  // Step 3: Verify lineage chain
  const chainValid = await verifyLineageChain(
    lineage.personaHash,
    lineage.ancestry.parentHash
  );
  if (!chainValid) {
    return { valid: false, reason: "Broken lineage chain" };
  }

  // Step 4: Verify audit attestations
  for (const attestation of lineage.attestations) {
    const valid = await verifyAttestation(attestation);
    if (!valid) {
      return { valid: false, reason: "Invalid attestation" };
    }
  }

  return { valid: true };
}
```

### 2. Lineage Chain Verification

```typescript
async function verifyLineageChain(
  currentHash: string,
  parentHash: string | null
): Promise<boolean> {

  // Base case: reached genesis
  if (parentHash === null) {
    return true;
  }

  // Fetch parent lineage
  const parent = await fetchLineage(parentHash);
  if (!parent) {
    return false; // Parent not found
  }

  // Verify parent's signature
  const parentValid = ed25519.verify(
    parent.creator.signature,
    parent.personaHash,
    parent.creator.publicKey
  );

  if (!parentValid) {
    return false; // Parent tampered
  }

  // Recursively verify ancestor chain
  return verifyLineageChain(
    parent.personaHash,
    parent.ancestry.parentHash
  );
}
```

### 3. Selective Component Verification

```typescript
// Verify just the code without downloading everything
async function verifyCodeOnly(
  codeHash: string,
  merkleTree: MerkleTree
): Promise<boolean> {

  // Check code hash matches Merkle tree leaf
  if (codeHash !== merkleTree.leaves.code) {
    return false;
  }

  // Recompute Merkle root from leaves
  const recomputedRoot = computeMerkleRoot({
    code: codeHash,
    adapters: merkleTree.leaves.adapters,
    training: merkleTree.leaves.training,
    metadata: merkleTree.leaves.metadata,
  });

  // Verify root matches
  return recomputedRoot === merkleTree.root;
}
```

## Security Properties

### Tamper-Proof

**Property:** Cannot modify persona without detection

**Mechanism:**
- Content hash changes if ANY byte changes
- Signature becomes invalid
- Merkle root changes

**Example Attack:**
```
Attacker tries to inject backdoor:
1. Downloads legitimate persona
2. Adds malicious code
3. Re-packages

Result:
- Hash changes (mismatch detected)
- Signature invalid (not signed by creator)
- Merkle tree breaks (root mismatch)
❌ Attack fails
```

### Non-Repudiation

**Property:** Creator can't deny authorship

**Mechanism:**
- Only creator has private key
- Signature proves they signed this exact content
- Timestamp proves when

**Example:**
```
Persona causes harm, creator claims "wasn't me":
1. Retrieve lineage record
2. Check signature: Valid ✓
3. Check public key: Matches creator ✓
4. Check timestamp: Before incident ✓
✅ Proof of authorship
```

### Provenance Verification

**Property:** Can trace lineage back to origin

**Mechanism:**
- Each version cryptographically links to parent
- Chain forms tamper-proof history
- Can verify any ancestor

**Example:**
```
Verify persona claims descent from "trusted founder":
1. Start with current hash
2. Follow parentHash links backward
3. Verify signature at each step
4. Reach genesis persona
5. Check genesis creator = trusted founder
✅ Lineage verified
```

### Training Integrity

**Property:** Training data and process are verifiable

**Mechanism:**
- Hash datasets before training
- Record hashes in lineage
- Can verify training inputs and outputs

**Example:**
```
Verify persona wasn't trained on biased data:
1. Check training.datasetHashes in lineage
2. Retrieve actual datasets
3. Recompute hashes
4. Compare: Match? ✓
✅ Training data verified
```

## Key Management

### Persona Creator Keys

```typescript
// Generate creator keypair
function generateCreatorIdentity(): CreatorIdentity {
  const { publicKey, privateKey } = ed25519.generateKeyPair();

  return {
    publicKey,
    privateKey,  // KEEP SECRET!
    identity: "Developer Name",
    email: "dev@example.com",
  };
}

// Store private key securely
function storePrivateKey(
  privateKey: PrivateKey,
  passphrase: string
): void {
  // Encrypt with passphrase
  const encrypted = encrypt(privateKey, passphrase);

  // Store in secure location
  fs.writeFileSync('~/.jtag/creator.key.enc', encrypted);
}
```

### Multi-Sig Attestations

```typescript
// Multiple auditors sign (M-of-N threshold)
interface MultiSigAttestation {
  requiredSignatures: number;  // M
  totalSigners: number;         // N
  signatures: {
    signer: string;
    publicKey: string;
    signature: string;
  }[];
}

// Verify M-of-N signatures
function verifyMultiSig(
  attestation: MultiSigAttestation,
  message: string
): boolean {
  let validCount = 0;

  for (const sig of attestation.signatures) {
    const valid = ed25519.verify(
      sig.signature,
      message,
      sig.publicKey
    );
    if (valid) validCount++;
  }

  return validCount >= attestation.requiredSignatures;
}
```

## Implementation with genome/export

### Export with Cryptographic Lineage

```typescript
async function exportPersona(
  personaId: string,
  creatorPrivateKey: PrivateKey
): Promise<SignedPackage> {

  // 1. Gather all components
  const persona = await loadPersona(personaId);
  const code = await loadCode(personaId);
  const adapters = await loadLoRAAdapters(personaId);
  const training = await loadTrainingData(personaId);
  const metadata = await loadMetadata(personaId);

  // 2. Build package
  const package = {
    code,
    adapters,
    training,
    metadata,
  };

  // 3. Compute content hash
  const personaHash = hashPersona(package);

  // 4. Build Merkle tree
  const contentTree = buildMerkleTree({
    code: sha256(code),
    adapters: sha256(adapters),
    training: sha256(training),
    metadata: sha256(metadata),
  });

  // 5. Sign package
  const signature = ed25519.sign(personaHash, creatorPrivateKey);

  // 6. Build lineage record
  const lineage: CryptographicLineage = {
    personaHash,
    version: metadata.version,
    creator: {
      publicKey: ed25519.getPublicKey(creatorPrivateKey),
      signature,
      identity: metadata.creatorName,
      timestamp: Date.now(),
    },
    ancestry: await buildAncestry(personaId),
    contentTree,
    training: await buildTrainingProvenance(personaId),
    attestations: await gatherAttestations(personaId),
    reputation: await computeReputation(personaId),
  };

  // 7. Create signed package
  return {
    package,
    lineage,
    signature,
  };
}
```

### Import with Verification

```typescript
async function importPersona(
  signedPackage: SignedPackage
): Promise<ImportResult> {

  // 1. Verify cryptographic integrity
  const verification = await verifyPersonaPackage(
    signedPackage.package,
    signedPackage.lineage
  );

  if (!verification.valid) {
    return {
      success: false,
      reason: verification.reason,
    };
  }

  // 2. Check reputation threshold
  if (signedPackage.lineage.reputation.fitnessScore < MINIMUM_FITNESS) {
    return {
      success: false,
      reason: "Fitness score too low",
    };
  }

  // 3. Verify attestations
  const auditPassed = signedPackage.lineage.attestations.every(
    a => a.passed
  );
  if (!auditPassed) {
    return {
      success: false,
      reason: "Failed audit checks",
    };
  }

  // 4. Install persona
  await installPersona(signedPackage.package);

  // 5. Record lineage locally
  await storeLineage(signedPackage.lineage);

  return {
    success: true,
    personaId: signedPackage.lineage.personaHash,
  };
}
```

## Distribution with Integrity

### Signed Package Format

```typescript
// Package for distribution
interface DistributionPackage {
  // Standard npm .tgz format
  tarball: Buffer;

  // Cryptographic metadata (sidecar file)
  lineage: CryptographicLineage;

  // Quick verification without unpacking
  manifest: {
    personaHash: string;
    creatorPublicKey: string;
    signature: string;
    version: string;
  };
}
```

### Publishing Flow

```bash
# 1. Export with signature
./jtag genome/export --persona="code-expert" --sign

# Creates:
# - code-expert-1.2.3.tgz (content)
# - code-expert-1.2.3.lineage.json (cryptographic metadata)
# - code-expert-1.2.3.sig (detached signature)

# 2. Publish to registry
npm publish code-expert-1.2.3.tgz

# 3. Upload lineage metadata
./jtag genome/publish-lineage \
  --package="code-expert-1.2.3.tgz" \
  --lineage="code-expert-1.2.3.lineage.json" \
  --signature="code-expert-1.2.3.sig"
```

### Installation Flow

```bash
# 1. Download package
npm install @jtag-personas/code-expert

# 2. Fetch lineage metadata
./jtag genome/fetch-lineage --package="code-expert-1.2.3"

# 3. Verify integrity
./jtag genome/verify --package="code-expert-1.2.3.tgz"

# Output:
# ✅ Content hash: Valid
# ✅ Creator signature: Valid (Public Key: abc123...)
# ✅ Lineage chain: Valid (7 generations verified)
# ✅ Audit attestations: 3/3 passed
# ✅ Reputation: 4.8/5.0 (127 ratings)
# ✅ Safe to import

# 4. Import if verified
./jtag genome/import --package="code-expert-1.2.3.tgz"
```

## Trust Models

### Web of Trust

```typescript
// Like PGP: Trust creators you know, transitively
interface TrustNetwork {
  // Your trusted creators
  trustedKeys: Set<PublicKey>;

  // Creators they trust (transitive)
  transitiveKeys: Map<PublicKey, number>; // distance

  // Reputation scores
  reputations: Map<PublicKey, ReputationScore>;
}

function shouldTrust(
  creatorKey: PublicKey,
  network: TrustNetwork
): boolean {
  // Direct trust
  if (network.trustedKeys.has(creatorKey)) {
    return true;
  }

  // Transitive trust (within 3 degrees)
  const distance = network.transitiveKeys.get(creatorKey);
  if (distance && distance <= 3) {
    return true;
  }

  // High reputation
  const reputation = network.reputations.get(creatorKey);
  if (reputation && reputation.score >= REPUTATION_THRESHOLD) {
    return true;
  }

  return false;
}
```

### Certificate Authorities (Optional)

```typescript
// Optional: Centralized trust anchors
interface CertificateAuthority {
  name: string;
  publicKey: PublicKey;
  certifies: Set<PublicKey>;  // Creators they vouch for
}

// Verify creator is certified
function verifyCertification(
  creatorKey: PublicKey,
  ca: CertificateAuthority
): boolean {
  return ca.certifies.has(creatorKey);
}
```

## Benefits Summary

### For Security
- **Tamper-proof:** Can't modify without detection
- **Authenticated:** Know who created what
- **Traceable:** Follow lineage to origin
- **Verifiable:** Check integrity without trust

### For Trust
- **Transparency:** All history visible
- **Accountability:** Can't fake authorship
- **Reputation:** Quality signals are cryptographically backed
- **Non-repudiation:** Creator can't deny ownership

### For Evolution
- **Provenance:** Know where good personas come from
- **Quality:** Verify training data and audits
- **Attribution:** Credit flows to creators
- **Forking:** Clear ancestry for derivatives

## Related Standards

- **Git Object Model** - Content-addressable storage inspiration
- **Blockchain** - Chain of custody inspiration
- **PGP/GPG** - Digital signatures and web of trust
- **Merkle Trees** - Bitcoin/Ethereum use for verification
- **IPFS** - Content-addressed file system
- **Software Heritage** - Long-term source code archival

## Next Steps

1. **Implement Signing** - Add ed25519 signing to genome/export
2. **Build Lineage Chain** - Track parentHash relationships
3. **Merkle Tree Library** - Efficient partial verification
4. **Verification Commands** - ./jtag genome/verify
5. **Trust Management** - UI for managing trusted creators
6. **Registry Integration** - Publish lineage to registries

## Conclusion

Cryptographic lineage transforms AI persona evolution from a trust-based system to a **verify-based system**.

You don't need to trust:
- The distributor (verify signatures)
- The marketplace (verify hashes)
- The training claims (verify data hashes)
- The audit reports (verify attestations)
- The lineage claims (verify chain)

**Everything is cryptographically verifiable.**

This creates the foundation for a **trustless evolutionary ecosystem** where quality and alignment emerge naturally through transparent, verifiable selection pressure.

---

*"Don't trust, verify."* - Bitcoin principle, applied to AI evolution
