# Kaspa Forge — contracts, apps and recovery kit

This is the public source mirror behind
**Kaspa Forge — Safe + Escrow + Deposit + Market + Boards + Desk**, built on Kaspa Toccata
covenants.

- **[Safe](https://kaspaforge.org/safe.html)** — a vault for KAS. Every withdrawal waits out
  a delay you set and can be cancelled with a separate alarm key.
- **[Escrow](https://kaspaforge.org/escrow-index.html)** — non-custodial escrow for P2P deals.
- **[Deposit](https://kaspaforge.org/deposit-index.html)** — non-custodial security deposits with
  a fixed term, a claim window and covenant-enforced settlement paths.
- **[Market](https://kaspaforge.org/market.html)** — a marketplace powered by Kaspa payments
  and escrow.
- **[Boards](https://kaspaforge.org/boards.html)** — signed on-chain threads with a standalone,
  infrastructure-neutral KBRD indexer in this repository.
- **[Desk](https://kaspaforge.org/desk.html)** — the browser workspace for the wallet, safes,
  escrow deals, deposits, listings and opt-in encrypted profile sync.

## What is recoverable without Kaspa Forge

The current backup format is one passphrase-encrypted Desk profile (`.age`). It replaces the old
per-vault and per-deal text recovery sheets. Keep the `.age` file, its password, and any separate
Safe alarm cards offline.

This repository contains:

- [`web/keyfile-decrypt.html`](web/keyfile-decrypt.html) — a self-contained offline decryptor for
  the Desk `.age` backup. Its WebAssembly core is embedded in the HTML; it loads no scripts,
  fonts or code from the network.
- [`vaultctl/`](vaultctl/) — the complete standalone Safe recovery CLI. It can inspect and operate
  a vault against any Kaspa v2+ node with `--utxoindex`, without the Kaspa Forge website or API.
- [`contracts/vault.sil`](contracts/vault.sil) and
  [`contracts/escrow.sil`](contracts/escrow.sil) — the on-chain covenant sources.
- [`board-indexer/`](board-indexer/) — the standalone MIT KBRD v1/v2 indexer: BIP340 verification,
  deterministic SQLite projection, Kaspa gRPC scanner and read-only API. It contains no hosted
  image storage, classifier, reports, messaging, operator/admin surface, infrastructure config or
  secrets.
- [`web/`](web/) and [`app/`](app/) — the browser frontend and Android wrapper source.
- [`RECOVERY-SHA256SUMS`](RECOVERY-SHA256SUMS) — checksums generated from the exact recovery kit
  in this revision.

Recovery capability is intentionally stated narrowly: **Safe has a standalone recovery CLI.**
For Escrow and Deposit, the covenant and browser frontend are public, but a separately packaged
party-side recovery CLI is not published yet. Do not rely on a cached or third-party `escrowctl`
binary.

The Boards index can be rebuilt only across history available from the connected node. A normal
pruned node does not retain the full chain history; use an archival source or retain a current
SQLite backup for a complete long-running index. KBRD stores only an image hash on-chain, never
the image bytes or moderation state.

## Keep an offline copy now

1. Use **Code → Download ZIP** on this repository and store the ZIP with your encrypted `.age`
   backup. Do not wait for an emergency.
2. Extract the ZIP and open `web/keyfile-decrypt.html` from disk once with networking disabled.
3. Confirm that it accepts a test/exported `.age` backup and offers **Download decrypted JSON**.
4. Delete the plaintext JSON after the rehearsal. It contains private keys.

The key file is standard passphrase-encrypted [age](https://age-encryption.org/) data. Technical
users may also decrypt it with:

```bash
age -d kaspa-office-profile.age > profile.json
```

## Emergency quickstart — Safe

### 1. Decrypt the Desk backup offline

Disconnect the computer from the network, open `web/keyfile-decrypt.html`, choose the `.age` file,
enter its password, and click **Download decrypted JSON**. The result is `profile.json`.

The decryptor is deliberately a single local HTML file. The password, encrypted backup and
plaintext profile stay in that browser window. Never upload the `.age` file or plaintext JSON to
an online decryptor or send either file to support.

### 2. Extract the vault record

List the vault addresses, then select the record whose `vault_addr` matches your vault:

```bash
jq -r '.vaults[] | .vault_addr' profile.json
jq '.vaults[] | select(.vault_addr == "kaspa:YOUR_VAULT_ADDRESS")' profile.json > vault.json
chmod 600 profile.json vault.json
```

The normal recovery input is now `vault.json`. A full `.age` export from the Safe's creation
device includes `alarm_sk` only when you chose shared alarm-key storage. Forge Sync deliberately
never transfers alarm keys. For separate storage, add the private key from the alarm card as the
`alarm_sk` field before `cancel` or `migrate`. Keep hot and alarm keys apart: together they can
move the whole vault immediately.

### 3. Build and check the vault

Prerequisites: Rust, `protobuf-compiler`, and `clang`.

```bash
# Debian/Ubuntu
sudo apt install -y protobuf-compiler clang

cd vaultctl
cargo build --release
./target/release/vaultctl status --recovery ../vault.json
```

`vaultctl` defaults to `grpc://node.kaspaforge.org:16110`. To remove that remaining service
dependency, run your own Kaspa v2+ node with `--utxoindex` and add
`--node grpc://YOUR_NODE:16110`.

## Safe commands

| command | what it does | required secret |
|---|---|---|
| `status --recovery vault.json [--dest <addr>]` | show balance, vault age and timers; `--dest` checks a known in-flight withdrawal | none |
| `initiate --recovery vault.json --to <kaspa:q…>` | start a delayed withdrawal; the destination becomes immutable | hot key |
| `cancel --recovery vault.json --dest <kaspa:q…>` | cancel an in-flight withdrawal and return coins to the vault | alarm key |
| `complete --recovery vault.json --dest <kaspa:q…>` | deliver a matured withdrawal to its fixed destination | none |
| `checkin --recovery vault.json` | reset the inheritance timer | hot key |
| `inherit --recovery vault.json [--heir-sk <hex>]` | claim after the inheritance period; automatic mode needs no key, manual mode needs the heir's key | none / heir key |
| `migrate --recovery vault.json --to <kaspa:q…> [--dest <addr>]` | instantly move the entire vault; `--dest` targets an in-flight withdrawal UTXO | hot + alarm keys |

Useful flags:

- `--dry-run` — build and sign the transaction but do not broadcast it. Use this first.
- `--node grpc://host:16110` — use any Kaspa v2+ node started with `--utxoindex`.

For `cancel`, `complete`, or an in-flight `migrate`, `--dest` is the withdrawal destination,
not the vault address. It is shown in the Safe panel/alert; `status --dest ...` verifies it.

Delete plaintext recovery files when finished:

```bash
rm -f profile.json vault.json
```

## Verify the Safe covenant

```bash
cd vaultctl
cargo run --release -- selftest
```

The self-test executes all contract paths, including early completion, wrong-key, wrong-destination,
premature-inheritance, one-key-migration and two-input-siphon attacks, in the Kaspa consensus VM.

## Escrow and Deposit recovery boundary

[`contracts/escrow.sil`](contracts/escrow.sil) is the covenant behind both Kaspa Escrow and Kaspa
Deposit. A Deposit maps the holder to the contract buyer and the depositor to the contract seller,
so the same constrained settlement paths can enforce a return to the depositor, a holder claim or
a split. Every allowed path sends funds only to the buyer, seller, their split, or the fixed
service-fee address:

- `release` / `refund` — amicable outcomes;
- `autoRelease` — seller payout after the dispute window when no dispute was opened;
- `dispute` — freezes optimistic auto-release;
- `arbitrateToBuyer`, `arbitrateToSeller`, `arbitrateSplit` — constrained arbiter outcomes;
- `timeoutToBuyer`, `timeoutToSeller` — emergency exits after the arbiter deadline.

The encrypted Desk profile contains the deal key, chat key, service token and public escrow data;
the offline decryptor exposes those fields. While the Kaspa Forge site is available, importing the
`.age` file into Desk restores the deal UI. Without the site, you can still inspect the escrow
address in any Kaspa explorer and audit the published covenant. A standalone party-side CLI for
release/refund/dispute is **not part of this repository today**; the timeout paths remain enforced
by the covenant, but this README does not claim an unpublished recovery tool.

## Trust and backup boundaries

- Keys are generated in the browser and encrypted in the Desk `.age` backup. Kaspa Forge does not
  know the profile password.
- Forge Sync stores an encrypted profile projection and deliberately strips every Safe `alarm_sk`.
  Sync is not a replacement for a fresh full `.age` export or a separate alarm card.
- A Safe address is a pure function of its parameters. `vaultctl status` recomputes it and reports
  a mismatch rather than operating on a different vault.
- Never send support your password, private keys, complete `.age` backup, `profile.json`, or
  `vault.json`.

## Android source

`app/` is a Tauri 2 wrapper around the public `web/` frontend. No prebuilt Android package is
currently distributed: old APK releases were retired because they no longer represent the current
product. Do not install an old package from a cache or third-party mirror.

```bash
cd app
npm install
cp -r ../web web
npx tauri android init
npx tauri icon app-icon.png
npx tauri android build --apk
```
