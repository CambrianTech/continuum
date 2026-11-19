# Rust ORM architecture

The ORM owns entity schemas, queries, durable representations and storage migrations. The [engine manual](../architecture/CONTINUUM-ENGINE.md) owns execution, resource scheduling and mind residency.

## Request routing

External callers pass an opaque database handle to a data or vector command. The host resolves that handle against its storage configuration. The command handler decodes its typed parameters and delegates to the storage adapter. Rust callers working with typed entities can use the ORM store directly. This keeps backend paths and SQL inside their owners.

| Change | Implementation owner |
|---|---|
| Command parameters, handle resolution and dispatch | [DataModule](../../core/continuum-store/src/data.rs) |
| Entity identity, base fields and registration | [Entity registry](../../core/continuum-kernel/src/orm/entity.rs) |
| Filters, ordering and query construction | [Query module](../../core/continuum-kernel/src/orm/query.rs) |
| Backend operations and their results | [StorageAdapter](../../core/continuum-kernel/src/orm/adapter.rs) |
| Entity operations and commit helpers | [ORM exports](../../core/continuum-kernel/src/orm/mod.rs) |

## Schema and row decoding

Declare the field representation on the entity. A JSON string scalar includes its JSON quotes. The staged-credit role `Owner` is stored as `"Owner"`. Native binary data stays in its declared binary representation. A worker refers to a large native artifact through a typed descriptor.

Readers use `DataRecord::decode` to interpret stored rows. A strict decode runs first. Older rows may have NULL in columns added after the row was written; the shared decoder allows declared defaults to supply missing fields. A required field without a valid value still fails. A schema change must distinguish intentional nullability from a migration default. [Row decoding](../../core/continuum-kernel/src/orm/record_decode.rs)

## Database concurrency

The connection manager keys pools by database path, creates them lazily and tracks idle and least-recently-used pools. Keep pool sizing and eviction in that owner. Independent database files can progress concurrently. Within one SQLite database, WAL permits concurrent readers while one write transaction at a time can modify the file. Keep the transaction scoped to the data that must change atomically. [Connection manager](../../core/continuum-kernel/src/orm/connection_manager.rs)

## Migration and verification

A representation change needs an explicit migration for affected existing rows. Select by schema and old representation. Preserve identity, logical values and unrelated fields. Verify rollback, retry after interruption, and typed reads from a reopened database. Put per-node counts and timings in the deployment receipt.

**Proposed contract for durable subscribers.** Scope progress to a consumer and stream. Commit an effect and its cursor together when both share a database transaction. For external effects, reconcile retries against a durable operation receipt. Replay a crash before commit, a crash after commit, and duplicate delivery after a successful commit whose acknowledgement was lost. The learning subscriber must recover the exact served turn for an accepted review.

## Design history

The original plan includes the TypeScript migration sequence, operation inventory, performance targets and historical measurements. Its implementation sketches and dated progress lists are preserved in the [versioned migration plan](https://github.com/CambrianTech/continuum/blob/cd4d7fccfe0e9cd92ff065a67bada44f04f66f1a/docs/infrastructure/RUST-ORM-ARCHITECTURE.md).
