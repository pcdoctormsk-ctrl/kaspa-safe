#[cfg(feature = "node")]
use anyhow::{bail, Context, Result};
#[cfg(feature = "node")]
use kbrd_indexer::{api, node, ProcessConfig, Store};
#[cfg(feature = "node")]
use std::{net::SocketAddr, sync::Arc, time::Duration};

#[cfg(feature = "node")]
#[derive(Debug)]
struct Config {
    node: String,
    db: String,
    listen: SocketAddr,
    start_daa: u64,
    poll_secs: u64,
    bump_limit: i64,
    pending_ttl_daa: u64,
    pending_max: usize,
}

#[cfg(feature = "node")]
impl Config {
    fn parse() -> Result<Self> {
        let mut config = Self {
            node: "grpc://127.0.0.1:16110".into(),
            db: "kbrd-index.sqlite".into(),
            listen: "127.0.0.1:8788".parse().unwrap(),
            start_daa: 0,
            poll_secs: 3,
            bump_limit: 300,
            pending_ttl_daa: 864_000,
            pending_max: 10_000,
        };
        let mut arguments = std::env::args().skip(1);
        while let Some(flag) = arguments.next() {
            let mut value = || {
                arguments
                    .next()
                    .with_context(|| format!("missing value for {flag}"))
            };
            match flag.as_str() {
                "--node" => config.node = value()?,
                "--db" => config.db = value()?,
                "--listen" => {
                    config.listen = value()?.parse().context("invalid --listen address")?
                }
                "--start-daa" => {
                    config.start_daa = value()?.parse().context("invalid --start-daa")?
                }
                "--poll-secs" => {
                    config.poll_secs = value()?.parse().context("invalid --poll-secs")?
                }
                "--bump-limit" => {
                    config.bump_limit = value()?.parse().context("invalid --bump-limit")?
                }
                "--pending-ttl-daa" => {
                    config.pending_ttl_daa =
                        value()?.parse().context("invalid --pending-ttl-daa")?
                }
                "--pending-max" => {
                    config.pending_max = value()?.parse().context("invalid --pending-max")?
                }
                "--help" | "-h" => {
                    println!(
                        "kbrd-indexer [--node URL] [--db PATH] [--listen ADDR]\n\
                         \x20             [--start-daa N] [--poll-secs N] [--bump-limit N]\n\
                         \x20             [--pending-ttl-daa N] [--pending-max N]"
                    );
                    std::process::exit(0);
                }
                _ => bail!("unknown argument {flag}; try --help"),
            }
        }
        if config.poll_secs == 0 || config.bump_limit < 1 || config.pending_max == 0 {
            bail!("poll-secs, bump-limit and pending-max must be positive");
        }
        Ok(config)
    }
}

#[cfg(feature = "node")]
#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::parse()?;
    let store = Arc::new(Store::open(&config.db)?);
    let client = Arc::new(node::connect(&config.node).await?);
    let node_config = node::NodeConfig {
        endpoint: config.node.clone(),
        start_daa: config.start_daa,
        poll_interval: Duration::from_secs(config.poll_secs),
        process: ProcessConfig {
            bump_limit: config.bump_limit,
            pending_ttl_daa: config.pending_ttl_daa,
            pending_max: config.pending_max,
        },
    };
    tokio::spawn(node::run(client, store.clone(), node_config));
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("bind read API to {}", config.listen))?;
    eprintln!("read API listening on http://{}", config.listen);
    axum::serve(listener, api::router(store))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(not(feature = "node"))]
fn main() {
    eprintln!("the kbrd-indexer binary requires the default `node` feature");
    std::process::exit(2);
}
