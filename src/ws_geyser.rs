use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{select, time::sleep};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::geyser_stats::SharedGeyserStats;

pub struct WsGeyser {
    inner: Arc<WsGeyserInner>,
}

struct WsGeyserInner {
    endpoint_with_key: String,
    seen_slots: DashMap<u64, bool>,
    last_slot: AtomicU64,
    stats: SharedGeyserStats,
}

#[derive(Serialize, Deserialize)]
struct SubscribeMessage {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<serde_json::Value>,
}

impl WsGeyser {
    pub fn new(endpoint_with_key: String, stats: SharedGeyserStats) -> Self {
        let inner = Arc::new(WsGeyserInner {
            endpoint_with_key,
            last_slot: AtomicU64::new(0),
            seen_slots: DashMap::new(),
            stats,
        });
        let ws_geyser = Self { inner };
        ws_geyser.poll_blocks();
        ws_geyser
    }

    fn poll_blocks(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = connect_and_subscribe(&inner).await {
                    error!("Error in WebSocket connection: {}", e);
                    sleep(Duration::from_secs(1)).await;
                }
            }
        });
    }
}

async fn connect_and_subscribe(
    inner: &WsGeyserInner,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = connect_async(&inner.endpoint_with_key).await?;
    let (mut write, mut read) = ws_stream.split();

    info!("WebSocket is open");

    // Send initial subscription request
    send_request(&mut write).await?;

    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        select! {
            _ = ping_interval.tick() => {
                if let Err(e) = write.send(Message::Ping(vec![])).await {
                    error!("Failed to send ping: {}", e);
                    return Err(Box::new(e));
                }
                info!("Ping sent");
            }
            msg = read.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<serde_json::Value>(&text) {
                        Ok(json) => {
                            if let Some(params) = json.get("params") {
                                if let Some(result) = params.get("result") {
                                    if let Some(slot) = result.get("slot").and_then(|s| s.as_u64()) {
                                        if !inner.seen_slots.contains_key(&slot) {
                                            inner.seen_slots.insert(slot, true);
                                            let current_last_slot = inner.last_slot.load(Ordering::Relaxed);
                                            if slot > current_last_slot {
                                                inner.last_slot.store(slot, Ordering::Relaxed);
                                                info!("WS-GEYSER: new slot: {}", slot);
                                                inner.stats.lock().await.add_ws_slot(slot);
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        Err(e) => error!("Failed to parse JSON: {}", e),
                    }
                }
                Some(Ok(Message::Binary(data))) => info!("Received binary data: {:?}", data),
                Some(Ok(Message::Ping(_))) => {
                    if let Err(e) = write.send(Message::Pong(vec![])).await {
                        error!("Failed to send pong: {}", e);
                        return Err(Box::new(e));
                    }
                }
                Some(Ok(Message::Pong(_))) => info!("Received pong"),
                Some(Ok(Message::Close(_))) => {
                    info!("WebSocket is closed");
                    return Ok(());
                }
                Some(Err(e)) => {
                    error!("WebSocket error: {}", e);
                    return Err(Box::new(e));
                }
                _ => return Ok(()),
            }
        }
    }
}

async fn send_request(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let subscribe_msg = SubscribeMessage {
        jsonrpc: "2.0".to_string(),
        id: generate_random_id(),
        method: "transactionSubscribe".to_string(),
        params: vec![
            json!({
                "accountInclude": ["Vote111111111111111111111111111111111111111"]
            }),
            json!({
                "commitment": "processed",
                "encoding": "jsonParsed",
                "transactionDetails": "full",
                "showRewards": true,
                "maxSupportedTransactionVersion": 0
            }),
        ],
    };

    let json_msg = serde_json::to_string(&subscribe_msg)?;
    write.send(Message::Text(json_msg)).await?;
    info!("Subscription request sent");

    Ok(())
}

fn generate_random_id() -> u64 {
    let mut rng = rand::thread_rng();
    rng.next_u64()
}
