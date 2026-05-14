use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use fedimint_api_client::api::DynGlobalApi;
use fedimint_connectors::ConnectorRegistry;
use fedimint_core::Amount;
use fedimint_core::core::{DynInput, DynOutput, ModuleInstanceId};
use fedimint_core::encoding::Encodable;
use fedimint_core::module::registry::ModuleDecoderRegistry;
use fedimint_core::secp256k1::{self, Secp256k1};
use fedimint_core::transaction::{Transaction, TransactionSignature};
use fedimint_core::util::SafeUrl;
use fedimint_core::PeerId;
use fedimint_falcon_client::{account_id_from_key, build_register_tx, build_transfer_tx_falcon};
use fedimint_falcon_common::{FalconInput, FalconOutput};
use pqcrypto_falcon::falcon512;
use pqcrypto_traits::sign::PublicKey as _;
use rand::Rng;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use tracing::info;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Scheme {
    Falcon512,
    Ed25519,
}

impl std::fmt::Display for Scheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scheme::Falcon512 => write!(f, "falcon512"),
            Scheme::Ed25519 => write!(f, "ed25519"),
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(name = "falcon-spammer", about = "Falcon-512 transaction spammer for hellas-falcon-bench")]
pub struct Args {
    /// WebSocket URL of any federation guardian, e.g. ws://127.0.0.1:8174
    #[arg(long, default_value = "ws://127.0.0.1:8174")]
    pub federation_url: String,

    /// Target transactions per second
    #[arg(long, default_value_t = 50)]
    pub tps: u64,

    /// How long to run (seconds)
    #[arg(long, default_value_t = 30)]
    pub duration: u64,

    /// Signature scheme to use
    #[arg(long, default_value = "falcon512")]
    pub scheme: Scheme,

    /// Federation module instance ID for the falcon module
    #[arg(long, default_value_t = 0)]
    pub module_instance_id: u16,

    /// Amount in millisatoshis per transaction
    #[arg(long, default_value_t = 1000)]
    pub amount_msat: u64,

    /// Maximum transactions in flight at once (default: 4× TPS)
    #[arg(long)]
    pub max_inflight: Option<usize>,
}

/// Submit all registration transactions and wait for them to be accepted.
/// Capped at 200 concurrent submissions to avoid overwhelming the WS connection.
async fn register_keys(
    api: &DynGlobalApi,
    module_instance_id: ModuleInstanceId,
    keys: &[Vec<u8>],
) -> Result<()> {
    info!(count = keys.len(), "Registering keypairs with federation");
    let semaphore = Arc::new(tokio::sync::Semaphore::new(200));
    let mut tasks: JoinSet<Result<()>> = JoinSet::new();

    for key_bytes in keys {
        let tx = build_register_tx(module_instance_id, key_bytes.clone());
        let api = api.clone();
        let permit = semaphore.clone().acquire_owned().await.expect("semaphore never closed");
        tasks.spawn(async move {
            let _permit = permit;
            match tokio::time::timeout(Duration::from_secs(30), api.submit_transaction(tx)).await {
                Ok(outcome) => {
                    outcome
                        .try_into_inner(&ModuleDecoderRegistry::default())
                        .map(|_| ())
                        .map_err(|e| anyhow::anyhow!("Registration rejected: {:?}", e))
                }
                Err(_) => Err(anyhow::anyhow!("Registration timed out")),
            }
        });
    }

    let mut ok = 0usize;
    let mut failed = 0usize;
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => ok += 1,
            Ok(Err(e)) => {
                failed += 1;
                tracing::warn!("Registration failed: {}", e);
            }
            Err(e) => {
                failed += 1;
                tracing::warn!("Registration task panicked: {}", e);
            }
        }
    }
    info!(ok, failed, "Registration complete");
    Ok(())
}

fn build_bench_tx_secp(
    module_instance_id: ModuleInstanceId,
    amount: Amount,
    keypair: &secp256k1::Keypair,
) -> Transaction {
    let secp = Secp256k1::new();
    let pub_key = keypair.public_key();
    let pub_key_bytes = pub_key.serialize();
    let account_id = account_id_from_key(&pub_key_bytes);

    let input = FalconInput {
        amount,
        unit: fedimint_core::module::AmountUnit::BITCOIN,
        pub_key,
        account_id,
    };
    let output = FalconOutput::Transfer {
        amount,
        unit: fedimint_core::module::AmountUnit::BITCOIN,
        recipient_account_id: account_id,
    };

    let inputs = vec![DynInput::from_typed(module_instance_id, input)];
    let outputs = vec![DynOutput::from_typed(module_instance_id, output)];
    let nonce: [u8; 8] = rand::thread_rng().r#gen();

    let txid = Transaction::tx_hash_from_parts(&inputs, &outputs, nonce);
    let msg = secp256k1::Message::from_digest(*txid.as_ref());
    let sig = secp.sign_schnorr(&msg, keypair);

    Transaction {
        inputs,
        outputs,
        nonce,
        signatures: TransactionSignature::NaiveMultisig(vec![sig]),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let module_instance_id = args.module_instance_id as ModuleInstanceId;
    let amount = Amount::from_msats(args.amount_msat);

    let max_inflight = args
        .max_inflight
        .unwrap_or_else(|| ((args.tps * 4) as usize).max(8));

    info!(
        tps = args.tps,
        duration_s = args.duration,
        max_inflight,
        scheme = %args.scheme,
        url = %args.federation_url,
        "Starting Falcon-512 spammer"
    );

    let connectors = ConnectorRegistry::build_from_client_defaults()
        .bind()
        .await?;
    let peer_url = SafeUrl::parse(&args.federation_url)?;
    let peers: BTreeMap<PeerId, SafeUrl> = [(PeerId::from(0), peer_url)].into();
    let api = DynGlobalApi::new(connectors, peers, None)?;

    let keypair_pool_size = ((args.tps as usize) * 2 + 200).min(5000);
    info!(size = keypair_pool_size, "Pre-generating keypairs");

    // Registration phase: submit each key once so the server has the account_id mapping.
    let tx_builder: Arc<dyn Fn(usize) -> Transaction + Send + Sync> = match args.scheme {
        Scheme::Falcon512 => {
            let keypairs: Arc<Vec<(falcon512::PublicKey, falcon512::SecretKey)>> =
                Arc::new((0..keypair_pool_size).map(|_| falcon512::keypair()).collect());

            // Register all keys with the federation before benchmarking.
            let key_blobs: Vec<Vec<u8>> = keypairs
                .iter()
                .map(|(pk, _)| pk.as_bytes().to_vec())
                .collect();
            register_keys(&api, module_instance_id, &key_blobs).await?;

            Arc::new(move |idx| {
                let kp = &keypairs[idx % keypairs.len()];
                build_transfer_tx_falcon(module_instance_id, amount, kp)
            })
        }
        Scheme::Ed25519 => {
            let secp = Secp256k1::new();
            let keypairs: Arc<Vec<secp256k1::Keypair>> = Arc::new(
                (0..keypair_pool_size)
                    .map(|_| secp256k1::Keypair::new(&secp, &mut rand::thread_rng()))
                    .collect(),
            );

            // Register secp keys too (stored as raw bytes; server just echoes them back for NaiveMultisig txs).
            let key_blobs: Vec<Vec<u8>> = keypairs
                .iter()
                .map(|kp| kp.public_key().serialize().to_vec())
                .collect();
            register_keys(&api, module_instance_id, &key_blobs).await?;

            Arc::new(move |idx| {
                let kp = &keypairs[idx % keypairs.len()];
                build_bench_tx_secp(module_instance_id, amount, kp)
            })
        }
    };

    let submitted = Arc::new(AtomicU64::new(0));
    let accepted = Arc::new(AtomicU64::new(0));
    let rejected = Arc::new(AtomicU64::new(0));
    let bytes_total = Arc::new(AtomicU64::new(0));
    let latency_sum_ms = Arc::new(AtomicU64::new(0));

    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_inflight));
    let deadline = Instant::now() + Duration::from_secs(args.duration);
    let mut tasks: JoinSet<()> = JoinSet::new();

    let tick_us = if args.tps == 0 { 1_000_000u64 } else { 1_000_000u64 / args.tps };
    let mut interval = tokio::time::interval(Duration::from_micros(tick_us));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut kp_idx: usize = 0;

    while Instant::now() < deadline {
        interval.tick().await;

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore never closed");

        let kp_idx_local = kp_idx;
        kp_idx = kp_idx.wrapping_add(1);

        let api = api.clone();
        let tx_builder = tx_builder.clone();
        let submitted = submitted.clone();
        let accepted = accepted.clone();
        let rejected = rejected.clone();
        let bytes_total = bytes_total.clone();
        let latency_sum_ms = latency_sum_ms.clone();

        tasks.spawn(async move {
            let _permit = permit;

            let tx = tx_builder(kp_idx_local);
            let tx_len = tx.consensus_encode_to_vec().len() as u64;

            submitted.fetch_add(1, Ordering::Relaxed);
            bytes_total.fetch_add(tx_len, Ordering::Relaxed);

            let t0 = Instant::now();
            match tokio::time::timeout(Duration::from_secs(10), api.submit_transaction(tx)).await {
                Ok(outcome) => {
                    match outcome.try_into_inner(&ModuleDecoderRegistry::default()) {
                        Ok(_) => {
                            accepted.fetch_add(1, Ordering::Relaxed);
                            latency_sum_ms
                                .fetch_add(t0.elapsed().as_millis() as u64, Ordering::Relaxed);
                        }
                        Err(_) => {
                            rejected.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                Err(_) => {
                    rejected.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }

    while tasks.join_next().await.is_some() {}

    let total = submitted.load(Ordering::Relaxed);
    let acc = accepted.load(Ordering::Relaxed);
    let rej = rejected.load(Ordering::Relaxed);
    let bytes = bytes_total.load(Ordering::Relaxed);
    let lat_sum = latency_sum_ms.load(Ordering::Relaxed);
    let avg_latency_ms = if acc > 0 { lat_sum / acc } else { 0 };

    println!("\n=== Falcon Spammer Results ===");
    println!("Scheme:          {}", args.scheme);
    println!("Target TPS:      {}", args.tps);
    println!("Achieved TPS:    {:.1}", total as f64 / args.duration as f64);
    println!("Duration:        {}s", args.duration);
    println!("Submitted:       {}", total);
    println!("Accepted:        {}", acc);
    println!("Rejected:        {}", rej);
    println!("Avg latency:     {}ms", avg_latency_ms);
    println!("Total bytes:     {} KB", bytes / 1024);
    println!("Bytes/tx:        {:.0}", if total > 0 { bytes as f64 / total as f64 } else { 0.0 });

    Ok(())
}
