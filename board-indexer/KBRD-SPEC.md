# KBRD envelope specification

This document specifies the bytes parsed by `kbrd-indexer`. Integers are unsigned. Multi-byte
lengths are little-endian.

## Common prefix and flags

| field | size | meaning |
|---|---:|---|
| magic | 4 | ASCII `KBRD` |
| version | 1 | `1` or `2` |
| flags | 1 | bit 0 = OP, bit 1 = image hash present; other bits are ignored |

KBRD v2 adds these fields immediately after `flags`:

| field | size | meaning |
|---|---:|---|
| identity mode | 1 | `0` = seed-derived anonymous key, `1` = stable trip key |
| recovery nonce | 16 | public salt for mode `0`; all zeroes for mode `1` |

Any other identity mode is invalid. Mode `1` with a non-zero nonce is invalid. KBRD v1 has neither
field and is represented with no recovery nonce.

## Content fields

| field | size | constraints |
|---|---:|---|
| board length | 1 | `1..=16` |
| board | variable | UTF-8 |
| parent txid | 32 | present only for a reply; absent for an OP |
| subject length | 1 | `0..=64` |
| subject | variable | UTF-8; must be empty for a reply |
| ephemeral public key | 32 | x-only secp256k1 public key |
| image SHA-256 | 32 | present only when flags bit 1 is set |
| body length | 2 | `1..=2048` |
| body | variable | UTF-8 |
| signature | 64 | BIP340 Schnorr signature |

The 64-byte signature must be the final field. Trailing bytes invalidate the envelope.

## Signature

Let `prefix` be every envelope byte from `magic` through the last byte of `body`, excluding the
signature. Verification is:

```text
digest = sha256(prefix)
valid = BIP340_verify(ephemeral_public_key, digest, signature)
```

No external domain tag or transaction ID is included. Because the carrying transaction is not
bound by the signature, indexers must reject a byte-identical envelope replayed under a different
transaction ID. The canonical owner is the first transaction in ascending `(DAA, txid)` order.

## Thread projection

- An OP's transaction ID is the thread ID.
- A reply's `parent txid` must name an indexed OP on the same board.
- Replies that arrive before their OP may be parked temporarily; they must be fully verified again
  before insertion.
- The OP has index `0`. Replies have indices `1..N` in ascending `(DAA, txid)` order.
- A transaction seen in multiple BlockDAG blocks uses its minimum observed DAA.

These rules make the final SQLite projection independent of block arrival and batch order within
the history observed by the indexer.

## Image boundary

KBRD contains only an optional SHA-256 commitment to an image. It does not contain the image bytes,
a URL, a storage provider, a moderation verdict, or a retention promise. Those are external
deployment concerns and are not part of this protocol or indexer.
