use log::info;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

struct SlotInfo {
    pub slot: u64,
    pub timestamp: Instant,
}

pub struct GeyserStats {
    grpc_slots: Vec<SlotInfo>,
    ws_slots: Vec<SlotInfo>,
}

impl GeyserStats {
    pub fn new() -> Self {
        Self {
            grpc_slots: Vec::new(),
            ws_slots: Vec::new(),
        }
    }

    pub fn add_grpc_slot(&mut self, slot: u64) {
        self.grpc_slots.push(SlotInfo {
            slot,
            timestamp: Instant::now(),
        });
    }

    pub fn add_ws_slot(&mut self, slot: u64) {
        self.ws_slots.push(SlotInfo {
            slot,
            timestamp: Instant::now(),
        });
    }

    pub fn last_grpc_slot(&self) -> Option<u64> {
        self.grpc_slots.last().map(|s| s.slot)
    }

    pub fn last_ws_slot(&self) -> Option<u64> {
        self.ws_slots.last().map(|s| s.slot)
    }

    pub fn last_slots_time_diff(&self) -> Option<Duration> {
        self.grpc_slots.last().and_then(|grpc| {
            self.ws_slots.last().map(|ws| {
                if grpc.timestamp > ws.timestamp {
                    grpc.timestamp.duration_since(ws.timestamp)
                } else {
                    ws.timestamp.duration_since(grpc.timestamp)
                }
            })
        })
    }
}

pub type SharedGeyserStats = Arc<Mutex<GeyserStats>>;

pub async fn analyze_geyser_stats(stats: SharedGeyserStats) {
    let mut interval = tokio::time::interval(Duration::from_secs(60)); // Analisa a cada minuto
    loop {
        interval.tick().await;
        let mut stats = stats.lock().await;

        // Calcular estatísticas
        let grpc_avg = calculate_average_latency(&stats.grpc_slots);
        let ws_avg = calculate_average_latency(&stats.ws_slots);
        let diff_avg = (grpc_avg as i64 - ws_avg as i64).abs() as u64;

        info!("gRPC average latency: {}ms", grpc_avg);
        info!("WebSocket average latency: {}ms", ws_avg);
        info!("Average difference: {}ms", diff_avg);

        // Limpar dados antigos
        stats.grpc_slots.clear();
        stats.ws_slots.clear();
    }
}

fn calculate_average_latency(slots: &[SlotInfo]) -> u64 {
    if slots.is_empty() {
        return 0;
    }
    let total_latency: u64 = slots
        .iter()
        .map(|s| s.timestamp.elapsed().as_millis() as u64)
        .sum();
    total_latency / slots.len() as u64
}
