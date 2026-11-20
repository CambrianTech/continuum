# Finding the lineage

[Parts](README.md)

Read the code as language. List the nouns (the types) and the verbs (the methods), and ask each word
for its parent, as language does: a truck is an automobile; a throw is an action.

- **Nouns that share a parent word share a base.** `KvSlotLease`, `EmbedLease` and `PortLease` are
  each a lease. Whisper, Kokoro and Piper are each a speech model, which is a model part.
- **Verbs that share a parent word share one implementation.** `backoff_after`, `retry_delay` and
  `backoff_delay` are each a backoff. Two `shutdown` loops over two registries are one walk.
- **Picture the system as animated objects:** things made of things, doing things. Their family tree
  is the module tree.

## A generation in Rust

- A capability is a trait; its parent capability is its supertrait (`Lifecycle: Part`).
- Shared behavior lives in a generic owner or a blanket implementation, so a leaf supplies only its
  hook (a model part supplies `load`, MODELS.md).
- A trait method has a default only when every implementer may safely inherit it. A required hook has
  no default.
- Mandatory sequencing goes in the generic owner's public method, with its state private, because a
  trait default can be overridden.
- An intermediate generation appears when several leaves share another invariant. The hardware or
  device specific algorithm stays at the leaf.

## The smells

- Two siblings declaring the same method: the method belongs to their parent.
- A leaf implementing straight from the root: a generation is missing.
- A parent looping over its children by hand: the walk belongs to the composite.
- State in a static or a global: a missing object (MODULE-ARCHITECTURE §9.3).
- A new trait beside an existing one with the same verbs: a second lineage. Extend the one that
  exists.
