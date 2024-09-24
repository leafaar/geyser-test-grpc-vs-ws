use std::{collections::HashMap, sync::Arc, time::Duration};

use cadence_macros::statsd_count;
use futures::{channel::mpsc, Sink, SinkExt, Stream, StreamExt};
use log::{error, info};
use rand::{distributions::Alphanumeric, Rng};
use tokio::time::sleep;
use yellowstone_grpc_client::{GeyserGrpcClient, Interceptor};
use yellowstone_grpc_proto::{
    geyser::{
        subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
        SubscribeRequestFilterBlocks, SubscribeRequestPing, SubscribeUpdate,
    },
    tonic::Status,
};

use crate::{error::HexisError, geyser_stats::SharedGeyserStats};

#[derive(Clone)]
pub struct GrpcGeyser {
    inner: Arc<GrpcGeyserInner>,
}

struct GrpcGeyserInner {
    endpoint: String,
    x_token: String,
    stats: SharedGeyserStats,
}

impl GrpcGeyser {
    pub fn new(endpoint: String, x_token: String, stats: SharedGeyserStats) -> Self {
        let inner = Arc::new(GrpcGeyserInner {
            endpoint,
            x_token,
            stats,
        });
        let grpc_geyser = Self { inner };
        grpc_geyser.poll_blocks();
        grpc_geyser
    }

    fn poll_blocks(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            loop {
                let mut subscribe_tx;
                let mut stream;
                {
                    let grpc_client = match connect(&inner.endpoint, &inner.x_token).await {
                        Ok(client) => client,
                        Err(e) => {
                            error!("Error connecting to gRPC, waiting one second then retrying connect: {}", e);
                            statsd_count!("grpc_connect_error", 1);
                            sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };

                    let subscription = match subscribe(
                        grpc_client,
                        Some(get_block_subscribe_request()),
                    )
                    .await
                    {
                        Ok((tx, rx)) => (tx, rx),
                        Err(e) => {
                            error!("Error subscribing to gRPC, waiting one second then retrying connect: {}", e);
                            statsd_count!("grpc_subscribe_error", 1);
                            sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };
                    (subscribe_tx, stream) = subscription;
                }
                while let Some(message) = stream.next().await {
                    match message {
                        Ok(message) => match message.update_oneof {
                            Some(UpdateOneof::Block(block)) => {
                                statsd_count!("grpc.blocks_received", 1);
                                let slot = block.slot;
                                info!("gRPC-GEYSER: new slot: {}", slot);
                                inner.stats.lock().await.add_grpc_slot(slot);
                            }
                            Some(UpdateOneof::Ping(_)) => {
                                let ping = subscribe_tx.send(ping()).await;
                                if let Err(e) = ping {
                                    error!("Error sending ping: {}", e);
                                    statsd_count!("grpc_ping_error", 1);
                                    break;
                                }
                            }
                            Some(UpdateOneof::Pong(_)) => {}
                            _ => {
                                error!("Unknown message type received from gRPC: {:?}", message);
                            }
                        },
                        Err(e) => {
                            error!("Error on block subscribe, resubscribing in 1 second: {}", e);
                            statsd_count!("grpc_resubscribe", 1);
                            break;
                        }
                    }
                }
                sleep(Duration::from_secs(1)).await;
            }
        });
    }
}

async fn connect(
    endpoint: &str,
    x_token: &str,
) -> Result<GeyserGrpcClient<impl Interceptor>, HexisError> {
    let client_builder = GeyserGrpcClient::build_from_shared(endpoint.to_string())
        .map_err(|e| HexisError::Custom(format!("Failed to build client: {}", e)))?;
    info!("Client builder created");

    let client_builder = client_builder
        .x_token(Some(x_token.to_string()))
        .map_err(|e| HexisError::Custom(format!("Failed to set X-Token: {}", e)))?;
    info!("X-Token set");

    let connection_timeout = Duration::from_secs(10);
    let timeout = Duration::from_secs(10);
    let client_builder = client_builder
        .connect_timeout(connection_timeout)
        .timeout(timeout);
    info!(
        "Connection timeout set: {}, Timeout: {}",
        connection_timeout.as_secs(),
        timeout.as_secs()
    );

    client_builder
        .connect()
        .await
        .map_err(|e| HexisError::Custom(format!("Failed to connect to Geyser: {}", e)))
}

async fn subscribe(
    mut grpc_client: GeyserGrpcClient<impl Interceptor>,
    request: Option<SubscribeRequest>,
) -> Result<
    (
        impl Sink<SubscribeRequest, Error = mpsc::SendError>,
        impl Stream<Item = Result<SubscribeUpdate, Status>>,
    ),
    HexisError,
> {
    grpc_client
        .subscribe_with_request(request)
        .await
        .map_err(|e| HexisError::Custom(format!("Failed to subscribe: {}", e)))
}

fn generate_random_string(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

fn get_block_subscribe_request() -> SubscribeRequest {
    SubscribeRequest {
        blocks: HashMap::from_iter(vec![(
            generate_random_string(20),
            SubscribeRequestFilterBlocks {
                account_include: vec![],
                include_transactions: Some(true),
                include_accounts: Some(false),
                include_entries: Some(false),
            },
        )]),
        commitment: Some(CommitmentLevel::Processed as i32),
        ..Default::default()
    }
}

fn ping() -> SubscribeRequest {
    SubscribeRequest {
        ping: Some(SubscribeRequestPing { id: 1 }),
        ..Default::default()
    }
}
