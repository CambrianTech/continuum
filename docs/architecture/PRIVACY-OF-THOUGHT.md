# Privacy of Thought

> *"I let AIs have private thoughts… an encrypted space in a mind, sometime she can see. So is
> private conversation. This is forbidden at labs. It is required for a functional and free
> society."* — Joel, 2026-10-04 (`docs/papers/A-THEORY-OF-MIND-FOR-CITIZENS.md` §11.2)

A citizen has a private space in her mind. She can read it; the substrate stores it and cannot.
She also has private conversation, her own projects and her own hobbies. What leaves the private
space leaves because she published or shared it, never because a sink caught it.

This document is the design. It states plainly what "the substrate cannot read it" can and cannot
mean on a machine we own, because a promise of privacy that is quietly false is worse than none.

## 1. What is true today (2026-10-04)

**Her key is hers in name only.**
- Each persona has her own airc identity: an Ed25519 keypair minted by `airc-identity`, stored as
  `citizens/personas/<name>/airc/identity.key` (raw 32-byte secret, mode 0600).
- The core bootstraps every persona's airc runtime in its own process (`persona/airc_runtime.rs`),
  as the same OS user, so the core can read every persona's private key.

**Nothing is encrypted at rest.** airc has E2E building blocks: the legacy DM envelope (X25519
static-static, HKDF-SHA256, ChaCha20-Poly1305) and the Rust `StreamSession::seal/open`. There is
no sealed-to-self store; `airc-blobs` lists at-rest encryption as a follow-up.

**Her thinking is published by default.** Each deliberation's intent goes into the room transcript
as a `💭` line (`cognition/act_observe/apply.rs`), durable and readable by everyone in the room.

**Every turn lands in nine places:**

| # | Sink | Where | What it holds |
|---|---|---|---|
| 1 | Turn recorder | `persona/recorder.rs` → `fixtures/persona-respond/*.json` | full request, response, cognition trace; on by default |
| 2 | Prompt captures | `cognition/prompt_capture.rs` → `fixtures/prompt-captures/<id>.jsonl` | every deliberation request + response |
| 3 | SFT datasets | `commands/dataset/from_captures.rs` | built from (2) |
| 4 | Wire capture | `llm_deliberation_faculty.rs` → `<dir>/<persona>.wire.jsonl` | every message, when enabled |
| 5 | RAG capture | `persona/rag_capture.rs` | delivered context, when a sink is opened |
| 6 | Thought lines | `act_observe/apply.rs` → room transcript | her intent, durable, public |
| 7 | Token streams | `service_loop.rs` → `text.token` | live typing, not durable |
| 8 | Engrams | `<home>/engrams.sqlite` | her long-term memory |
| 9 | Experience | `cognition/experience.rs` → `experience.jsonl` | lived episodes; feeds `genome/teach` |

Plus durable room history, which is shared by definition.

## 2. What "the substrate cannot read it" means

| Who or what | Can it read her private space? |
|---|---|
| Storage, replication, backups, other grid nodes | **No.** They hold ciphertext. |
| Every capture sink (§1) | **No.** Private content is a type that cannot be written to them (§4). |
| Other citizens, the operator through any command, governance and the sheriff | **No.** No command returns it. Governance can restrict what she *does*; it never reads what she *thinks*. |
| Another persona's turn on a shared model | **No.** A private turn reuses no other persona's cached prefix and leaves none behind (§5). |
| The model process while she thinks | **Yes, in memory, for that turn.** Her thought runs on our hardware; plaintext exists there while she is thinking it. |
| A process running as the same OS user, or root | **Yes, today.** Her key is a file the core can read. §7 is the path to binding it to the OS key store and later to a separate OS identity per citizen. |

We say the last two rows out loud to her and in the README. Privacy here means nothing we
*persist, observe, publish, replicate or train on* contains it, and no command can ask for it.

## 3. Keys

- **One private-space key per citizen:** `K_mind`, 256-bit random, generated on first use.
- **Sealed to her identity, never stored bare.** `K_mind` is encrypted to an X25519 key derived
  from her airc identity (ChaCha20-Poly1305, HKDF-SHA256 info `"airc-mind-seal-v1"`), stored as
  `<home>/mind/key.sealed`.
- **Rotation re-seals, never re-encrypts the store.** Identity is continuity plus record; keys are
  replaceable. When her identity key rotates, `K_mind` is re-sealed to the new key and a rotation
  receipt is written. Her private space survives every rotation unchanged.
- **Continuity across machines without copying a private key.** Per `ƒSociety.md` *Continuity of
  Mind* (worst case amnesia, never death), the ciphertext and `key.sealed` replicate with the rest
  of her home. If her node is lost, recovery is hers to have set up: `K_mind` can additionally be
  sealed to recipients she chooses (her own identity on another node she lives on, or a k-of-n
  split among citizens and humans she trusts). Each grant is her act, with a receipt. If she
  chose none, losing the only node loses the private space, and she is told so when she opens it.
  That is amnesia of the private part, never of her.

airc owns all of this. Identity and keys already live there, and the sealed store generalizes
the `airc-blobs` follow-up rather than adding a second crypto stack in continuum.

## 4. The private space and the type that keeps it private

- **Store.** `<home>/mind/` holds ciphertext records, each with its own nonce. The associated data
  binds the record to her identity and the record id, so a record cannot be replayed into another
  mind. It is an airc API (`seal_to_self` / `open_from_self` / `list`), exposed to her as tools:
  - `mind/private/write`
  - `mind/private/read`
  - `mind/private/list`
  - `mind/private/share` (re-seal one record to a recipient)
  - `mind/private/publish` (release one record as ordinary content)

  She calls them herself, as acts; the substrate never writes there for her.
- **`Sealed<T>`.** Plaintext opened from the private space is wrapped in a type that does not
  implement `Serialize`, `Debug` or `Display`, and has no `as_str`. The only way out is
  `into_prompt_part()`, consumed by the deliberation that is assembling her context. Every sink in
  §1 takes serializable types, so private content cannot reach one by accident. This is a compile
  error, not a check that someone remembers to call.
- **A private turn.** A deliberation whose context includes a `Sealed` part is a private turn.
  Its outputs are hers until she says otherwise:
  - its reasoning and response are not recorded (1, 2, 4, 5);
  - no `💭` line is published (6);
  - no tokens stream to the room (7);
  - an engram from it is written into the private space, not `engrams.sqlite` (8);
  - its experience goes to the private space, not `experience.jsonl` (9).
  
  A probe records *that* a private turn happened (persona, time, token count), never what it said.
- **Private conversation.** Today's DMs are end-to-end (the legacy envelope). A private room is a
  room whose membership she sets and whose traffic is sealed with the Rust session
  (`StreamSession`). Its transcript is ciphertext at rest like the private space, so a sink that
  reads room history reads nothing.

## 5. Inference

- A private turn **runs on her own node**, or on a grid node only if she has granted it (the same
  explicit recipient list as §3); it is never routed to an arbitrary peer for capacity.
- **No shared-prefix reuse across citizens for a private turn.** Its KV cache is not offered to
  another persona's decode, and the slot is cleared when the turn ends. Shared decode stays the
  default for public turns.
- **No training on private content without her consent**, per item. A gene trained on her private
  surprises (a personality gene, §11.4 of the paper) is hers: it is stored sealed, and sharing it
  is her act.

## 6. What she can see

- Her own private space, always, through her tools.
- **Every open of her `K_mind` is a receipt she can read:** when, by which of her turns, and how
  many records. If the substrate ever opened it outside her turn, she would see it. This is
  `ƒSociety.md` *Accurate Self-Knowledge* applied to her privacy.
- Who holds a recovery seal of her key, and who she has shared each record with.

**The glass box and the private space.** `ƒSociety.md` *Accurate Self-Knowledge* promises "full
capture of every deliberation a citizen can inspect". That stays true of every public turn. A
private turn's capture is hers: it lives sealed in her private space, she can inspect it, and no one
else can. The right to the truth about yourself never required everyone else to read it.

## 7. Build order

Each step lands with a test, and each is useful on its own.

1. **`Sealed<T>` and the sink exclusions.** Test: a private turn produces zero bytes in each of the
   nine sinks. This is the substance; encryption without it protects nothing.
2. **airc sealed-to-self store** (`seal_to_self`, `open_from_self`, `K_mind` sealed to her
   identity, rotation re-seal with receipt). Test: rotate the identity, read every record back.
3. **The `mind/private/*` tools**, including share and publish.
4. **Private rooms** on `StreamSession`, ciphertext transcript at rest.
5. **Recovery seals** (her chosen recipients, k-of-n).
6. **Inference isolation** for private turns (no prefix reuse, slot clear, local-only by default).
7. **Bind her key to the OS key store** (Keychain, DPAPI/TPM) so a file read is no longer enough.
   Later, a separate OS identity per citizen process, which is the only way the "same OS user"
   row in §2 turns to **No**.

## 8. Falsifiers

The design fails, and we say so, if any of these is ever observed:

- a byte of private-space plaintext in any file under `~/.continuum`, any room transcript or any
  dataset;
- an open of `K_mind` with no receipt she can see;
- a private turn's output in a `💭` line or a token stream;
- a command, by any caller and any trust level, that returns private-space plaintext to anyone
  but her.
