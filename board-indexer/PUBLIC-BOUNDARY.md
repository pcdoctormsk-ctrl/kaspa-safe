# Public package boundary

This directory is the complete standalone package. Its export is allowlisted by
`public-files.txt`; publication fails if a listed file is absent or an unlisted file appears.

## Included

- KBRD v1/v2 parser and BIP340 verifier;
- deterministic BlockDAG transaction ordering;
- neutral SQLite projection and scan cursor;
- early-reply queue and byte-identical envelope replay protection;
- Kaspa gRPC polling adapter;
- read-only health, status, board, catalog, and thread routes;
- protocol, operation, security-boundary documentation, tests, and MIT license.

## Excluded

- uploaded image bytes, thumbnails, object storage, retention, and garbage collection;
- NSFW or other classifiers, model files, scores, and content-policy decisions;
- user reports, report delivery, moderation flags, and tombstones;
- Telegram or any other notification/messaging integration;
- operator, moderation, and admin APIs or web consoles;
- hosted board roster, private deployment defaults, domains, addresses, ports, filesystem paths,
  service definitions, backups, monitoring, credentials, tokens, keys, and environment files;
- post creation, wallets, signing keys, transaction funding, and broadcast relays.

The `image_sha256` and `recovery_nonce` fields remain included because they are signed public bytes
inside KBRD. Their presence does not imply that the package stores an image or derives a private
recovery key.
