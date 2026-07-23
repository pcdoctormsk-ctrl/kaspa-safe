# Kaspa Forge — Independent Recovery

This repository is the recovery-only public source package for
[Kaspa Forge](https://kaspaforge.org). It lets users recover funds from an existing Kaspa Safe,
Escrow or Deposit without the Kaspa Forge website, hosted API or operator infrastructure.

It intentionally contains only:

- [`contracts/`](contracts/) — the published `vault.sil` and `escrow.sil` covenant sources;
- [`vaultctl/`](vaultctl/) — the standalone Safe recovery CLI;
- [`recovery-kit/`](recovery-kit/) — the Escrow and Deposit transaction core, `dealctl`, schemas,
  test vectors, EN/RU guides and self-contained offline `.age` decryptor;
- [`RECOVERY-SHA256SUMS`](RECOVERY-SHA256SUMS) — checksums for the root recovery boundary. The Deal
  Recovery Kit also has its complete package manifest at
  [`recovery-kit/RECOVERY-SHA256SUMS`](recovery-kit/RECOVERY-SHA256SUMS).

The website, blog, Desk/application source, Boards indexer, server, image storage, NSFW model,
reports, Telegram integration, operator/admin APIs, deployment configuration, infrastructure and
secrets are outside this repository by design.

## Keep an offline copy now

1. Use **Code → Download ZIP** and store the ZIP with your encrypted Desk `.age` backup. Do not
   wait for an emergency.
2. Verify `RECOVERY-SHA256SUMS`.
3. With networking disabled, open `recovery-kit/keyfile-decrypt.html` from disk and rehearse with
   a test/exported `.age` backup.
4. Delete the decrypted JSON after the rehearsal. It contains private keys.

The key file is standard passphrase-encrypted [age](https://age-encryption.org/) data. Technical
users may also decrypt it with:

```bash
age -d kaspa-office-profile.age > profile.json
```

Never upload the `.age` file or decrypted JSON to an online decryptor, and never send either one,
your password or a private key to support.

## Safe recovery

### Build `vaultctl`

Prerequisites: Rust, `protobuf-compiler`, and `clang`.

```bash
# Debian/Ubuntu
sudo apt install -y protobuf-compiler clang

cd vaultctl
cargo build --release --locked
./target/release/vaultctl status --recovery ../vault.json
```

`vaultctl` defaults to `grpc://node.kaspaforge.org:16110`. For independence from that public
front, run any compatible Kaspa v2+ node with `--utxoindex` and pass
`--node grpc://YOUR_NODE:16110`.

### Extract the Safe record offline

Decrypt the Desk backup on a disconnected computer, then select the record whose `vault_addr`
matches the vault:

```bash
jq -r '.vaults[] | .vault_addr' profile.json
jq '.vaults[] | select(.vault_addr == "kaspa:YOUR_VAULT_ADDRESS")' profile.json > vault.json
chmod 600 profile.json vault.json
```

An `.age` export from the Safe creation device includes `alarm_sk` only when shared alarm-key
storage was selected. Forge Sync deliberately does not transfer alarm keys. If the alarm key was
kept separately, add it as `alarm_sk` before `cancel` or `migrate`. Keep hot and alarm keys apart:
together they can move the entire vault immediately.

### Safe commands

| command | purpose | required secret |
|---|---|---|
| `status --recovery vault.json [--dest <addr>]` | show balance, age and timers | none |
| `initiate --recovery vault.json --to <kaspa:q…>` | start a delayed withdrawal | hot key |
| `cancel --recovery vault.json --dest <kaspa:q…>` | cancel an in-flight withdrawal | alarm key |
| `complete --recovery vault.json --dest <kaspa:q…>` | deliver a matured withdrawal | none |
| `checkin --recovery vault.json` | reset the inheritance timer | hot key |
| `inherit --recovery vault.json [--heir-sk <hex>]` | claim after the inheritance period | none / heir key |
| `migrate --recovery vault.json --to <kaspa:q…> [--dest <addr>]` | immediate full migration | hot + alarm keys |

Use `--dry-run` before broadcasting. For `cancel`, `complete`, or an in-flight `migrate`, `--dest`
is the fixed withdrawal destination, not the vault address.

Run the covenant self-test:

```bash
cd vaultctl
cargo run --release --locked -- selftest
```

## Escrow and Deposit recovery

`contracts/escrow.sil` backs both Kaspa Escrow and Kaspa Deposit. A Deposit maps the holder to the
contract buyer and the depositor to the contract seller. Every permitted path constrains funds to
the buyer, seller, their split, or the fixed service-fee address:

- `release` / `refund` — amicable outcomes;
- `autoRelease` — seller payout after the dispute window;
- `dispute` — freezes optimistic auto-release;
- `arbitrateToBuyer`, `arbitrateToSeller`, `arbitrateSplit` — constrained arbiter outcomes;
- `timeoutToBuyer`, `timeoutToSeller` — emergency exits after the arbiter deadline.

Build `dealctl` from the recovery-kit root:

```bash
cd recovery-kit
sha256sum -c RECOVERY-SHA256SUMS
cargo build --release --locked -p dealctl
./target/release/dealctl --help
```

Read the complete guides before using it:

- [`recovery-kit/recovery/README.md`](recovery-kit/recovery/README.md) — English;
- [`recovery-kit/recovery/README.ru.md`](recovery-kit/recovery/README.ru.md) — Russian.

The safe air-gapped boundary is:

1. Offline: decrypt, `extract`, `verify`, then create a public `watch.json`.
2. Online: use `status --watch` and `prepare --watch` to create `lines.json`.
3. Offline: sign the line package against the private recovery record.
4. Online: submit the signed transaction.

Only `watch.json`, `lines.json` and the signed transaction cross to the online host. The `.age`
backup, decrypted profile, recovery record and private key remain offline. Offline signing rejects
a line package whose network, product or deal ID does not match the recovery record.

## Trust boundary

- Keys originate client-side and are encrypted in the Desk `.age` backup.
- Forge Sync is not a replacement for a fresh full `.age` export or a separate Safe alarm card.
- `vaultctl` and `dealctl` recompute covenant identities and fail closed on mismatches.
- A service capability token is not needed for on-chain recovery.
- Build from reviewed source and verify the checksum manifests; do not trust cached or third-party
  binaries.
- Lost passwords and lost private keys cannot be recovered by Kaspa Forge.

Delete plaintext recovery material when finished:

```bash
rm -f profile.json vault.json deal-recovery.json
```
