use crate::{flatten_sorted, process_txs, ProcessConfig, Store};
use anyhow::{anyhow, Context, Result};
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode, RpcHash};
use std::{str::FromStr, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub endpoint: String,
    pub start_daa: u64,
    pub poll_interval: Duration,
    pub process: ProcessConfig,
}

pub async fn connect(endpoint: &str) -> Result<GrpcClient> {
    GrpcClient::connect_with_args(
        NotificationMode::Direct,
        endpoint.to_owned(),
        None,
        true,
        None,
        false,
        None,
        Default::default(),
    )
    .await
    .with_context(|| format!("connect to Kaspa node {endpoint}"))
}

fn start_hash(
    stored: Option<String>,
    sink: RpcHash,
    pruning_point: RpcHash,
    start_daa: u64,
) -> RpcHash {
    stored
        .and_then(|value| RpcHash::from_str(value.trim()).ok())
        .unwrap_or(if start_daa > 0 { pruning_point } else { sink })
}

pub async fn poll_once(client: &GrpcClient, store: &Store, config: &NodeConfig) -> Result<usize> {
    let dag = client
        .get_block_dag_info()
        .await
        .context("get block DAG info")?;
    let low = start_hash(
        store.state_get("last_hash")?,
        dag.sink,
        dag.pruning_point_hash,
        config.start_daa,
    );
    let response = match client.get_blocks(Some(low), true, true).await {
        Ok(response) => response,
        Err(error) => {
            store.state_set("last_hash", &dag.sink.to_string())?;
            return Err(anyhow!(
                "stored scan point {low} is unavailable ({error}); reset to current sink {}",
                dag.sink
            ));
        }
    };
    let blocks = response
        .blocks
        .iter()
        .map(|block| {
            (
                block.header.daa_score,
                block
                    .transactions
                    .iter()
                    .filter_map(|transaction| {
                        if !transaction.payload.starts_with(crate::envelope::MAGIC) {
                            return None;
                        }
                        let txid = transaction
                            .verbose_data
                            .as_ref()?
                            .transaction_id
                            .to_string();
                        Some((txid, transaction.payload.clone()))
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    let items = flatten_sorted(&blocks, config.start_daa);
    let stored = process_txs(store, &items, dag.virtual_daa_score, &config.process)?;
    if let Some(hash) = response.block_hashes.last() {
        store.state_set("last_hash", &hash.to_string())?;
        store.state_set("last_daa", &dag.virtual_daa_score.to_string())?;
    }
    Ok(stored)
}

pub async fn run(client: Arc<GrpcClient>, store: Arc<Store>, config: NodeConfig) {
    loop {
        match poll_once(&client, &store, &config).await {
            Ok(stored) if stored > 0 => eprintln!("indexed {stored} KBRD post(s)"),
            Ok(_) => {}
            Err(error) => eprintln!("scan error: {error:#}"),
        }
        tokio::time::sleep(config.poll_interval).await;
    }
}
