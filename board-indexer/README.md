# KBRD indexer

`kbrd-indexer` is a standalone, read-only indexer for signed KBRD board posts carried in Kaspa
transaction payloads. It verifies every envelope, projects the on-chain fields into SQLite, and
serves a small read-only HTTP API.

The package is intentionally deployment-neutral. It has no image storage, classifier, moderation
or report database, messaging integration, operator API, hosted endpoint, infrastructure path, or
secret configuration.

## What it indexes

- KBRD v1 and v2 envelopes;
- BIP340 signatures over `sha256(envelope_without_signature)`;
- boards discovered from valid OPs rather than a hosted board roster;
- threads and replies in deterministic `(DAA, txid)` order;
- public recovery nonce and image hash metadata present in the signed envelope;
- scan progress, early replies, and envelope replay protection.

The `image_sha256` column is only an on-chain commitment. This package never accepts, stores, or
serves image bytes.

## Run

Prerequisites are Rust, `protobuf-compiler`, `clang`, and a Kaspa node with its gRPC interface
enabled.

```bash
sudo apt install -y protobuf-compiler clang
cargo run --release -- \
  --node grpc://127.0.0.1:16110 \
  --db ./kbrd-index.sqlite \
  --listen 127.0.0.1:8788
```

The defaults are the values above. See all options with `cargo run --release -- --help`.

To scan the node's available history instead of starting from its current sink, pass a non-zero
protocol DAA:

```bash
cargo run --release -- --start-daa 123456789
```

On a fresh database, a non-zero `--start-daa` begins at the node's pruning point and drops blocks
below the requested DAA. With the default `--start-daa 0`, a fresh database starts at the current
sink and indexes new posts from that point onward.

## Read API

All routes are `GET`; the package has no write, moderation, operator, or admin route.

| route | result |
|---|---|
| `/healthz` | process health |
| `/v1/status` | post/thread/pending counts and scan cursor |
| `/v1/boards` | board slugs discovered from indexed OPs |
| `/v1/catalog?board=&limit=&offset=` | newest-bumped threads, optionally filtered by board |
| `/v1/thread/{op_txid}?limit=&offset=` | one thread in canonical post order |

The API returns the signed on-chain content as indexed. A public deployment should put its own
rate limiting and content policy in front of this neutral read API.

## Rebuild boundary

SQLite is a derived cache, but a rebuild can only see what the connected node can still serve.
An ordinary pruned node does not retain all historical blocks. To preserve a complete long-running
index, use an archival source or back up the SQLite database while the indexer is current.

Only KBRD envelope fields are reconstructible:

- post text, subject, board, public authorship key, recovery nonce, and image SHA-256 commitment
  are on-chain;
- image bytes, moderation decisions, reports, viewer preferences, and notification state are not
  on-chain and are outside this package.

## Security model

- Every length is checked before slicing untrusted transaction payloads.
- UTF-8 is decoded only after the BIP340 signature verifies.
- A byte-identical envelope rebroadcast under another transaction ID is rejected as a replay.
- Replies received before their OP are parked, then fully re-verified when the OP appears.
- Thread order and bump order are functions of `(DAA, txid)`, not arrival timing.
- The HTTP surface is read-only and binds to loopback by default.

See [KBRD-SPEC.md](KBRD-SPEC.md) for the wire format and [PUBLIC-BOUNDARY.md](PUBLIC-BOUNDARY.md)
for the explicit publication boundary.

## Tests

```bash
cargo test
```

The package is MIT licensed.
