use crate::envelope::{parse_and_verify, MAGIC};
use crate::store::{IngestOutcome, Store};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxItem {
    pub txid: String,
    pub daa: u64,
    pub payload: Vec<u8>,
}

pub type BlockBatch = (u64, Vec<(String, Vec<u8>)>);

#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub bump_limit: i64,
    pub pending_ttl_daa: u64,
    pub pending_max: usize,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            bump_limit: 300,
            pending_ttl_daa: 864_000,
            pending_max: 10_000,
        }
    }
}

pub fn flatten_sorted(blocks: &[BlockBatch], min_daa: u64) -> Vec<TxItem> {
    let mut best: HashMap<&str, (u64, &Vec<u8>)> = HashMap::new();
    for (daa, transactions) in blocks.iter().filter(|(daa, _)| *daa >= min_daa) {
        for (txid, payload) in transactions {
            match best.get(txid.as_str()) {
                Some((seen, _)) if *seen <= *daa => {}
                _ => {
                    best.insert(txid, (*daa, payload));
                }
            }
        }
    }
    let mut items: Vec<_> = best
        .into_iter()
        .map(|(txid, (daa, payload))| TxItem {
            txid: txid.to_owned(),
            daa,
            payload: payload.clone(),
        })
        .collect();
    items.sort_by(|left, right| {
        left.daa
            .cmp(&right.daa)
            .then_with(|| left.txid.cmp(&right.txid))
    });
    items
}

pub fn process_txs(
    store: &Store,
    items: &[TxItem],
    current_daa: u64,
    config: &ProcessConfig,
) -> Result<usize> {
    store.prune_pending(current_daa.saturating_sub(config.pending_ttl_daa) as i64)?;
    let mut stored = 0;
    for item in items {
        if item.payload.len() < MAGIC.len() || &item.payload[..MAGIC.len()] != MAGIC {
            continue;
        }
        let Ok(post) = parse_and_verify(&item.payload) else {
            continue;
        };
        let envelope_hash = hex::encode(Sha256::digest(&item.payload));
        if !store.note_envelope(&envelope_hash, &item.txid, item.daa as i64)? {
            continue;
        }
        match store.ingest(&item.txid, item.daa as i64, &post, config.bump_limit)? {
            IngestOutcome::Stored { is_op } => {
                stored += 1;
                if is_op {
                    stored += flush_pending(store, &item.txid, config)?;
                }
            }
            IngestOutcome::MissingParent => {
                let parent = hex::encode(post.parent_txid.expect("reply has parent"));
                store.park_pending(
                    &item.txid,
                    &parent,
                    item.daa as i64,
                    &item.payload,
                    current_daa as i64,
                    config.pending_max,
                )?;
            }
            IngestOutcome::Duplicate | IngestOutcome::Invalid => {}
        }
    }
    Ok(stored)
}

fn flush_pending(store: &Store, op_txid: &str, config: &ProcessConfig) -> Result<usize> {
    let mut stored = 0;
    for (txid, daa, payload) in store.take_pending(op_txid)? {
        let Ok(post) = parse_and_verify(&payload) else {
            continue;
        };
        if matches!(
            store.ingest(&txid, daa, &post, config.bump_limit)?,
            IngestOutcome::Stored { .. }
        ) {
            stored += 1;
        }
    }
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::test_support::signed_payload;
    use crate::envelope::VERSION_V2;

    #[test]
    fn canonical_order_collapses_dag_duplicates() {
        let blocks = vec![
            (201, vec![("same".into(), b"x".to_vec())]),
            (
                200,
                vec![("same".into(), b"x".to_vec()), ("z".into(), b"z".to_vec())],
            ),
            (10, vec![("old".into(), b"o".to_vec())]),
        ];
        let items = flatten_sorted(&blocks, 100);
        assert_eq!(
            items
                .iter()
                .map(|item| (item.daa, item.txid.as_str()))
                .collect::<Vec<_>>(),
            vec![(200, "same"), (200, "z")]
        );
    }

    #[test]
    fn early_reply_flush_and_replay_guard() {
        let store = Store::open(":memory:").unwrap();
        let op_txid = [0x77; 32];
        let reply = signed_payload(
            false,
            VERSION_V2,
            "any-board",
            Some(op_txid),
            "",
            "early",
            3,
            None,
        );
        let op = signed_payload(true, VERSION_V2, "any-board", None, "hello", "op", 4, None);
        let config = ProcessConfig::default();
        assert_eq!(
            process_txs(
                &store,
                &[TxItem {
                    txid: hex::encode([1; 32]),
                    daa: 50,
                    payload: reply
                }],
                100,
                &config
            )
            .unwrap(),
            0
        );
        assert_eq!(
            process_txs(
                &store,
                &[TxItem {
                    txid: hex::encode(op_txid),
                    daa: 60,
                    payload: op.clone()
                }],
                101,
                &config
            )
            .unwrap(),
            2
        );
        assert_eq!(
            store
                .thread(&hex::encode(op_txid), 100, 0)
                .unwrap()
                .unwrap()
                .post_count,
            2
        );

        assert_eq!(
            process_txs(
                &store,
                &[TxItem {
                    txid: hex::encode([9; 32]),
                    daa: 70,
                    payload: op
                }],
                102,
                &config
            )
            .unwrap(),
            0,
            "byte-identical envelope under a new txid is not a new post"
        );
    }
}
