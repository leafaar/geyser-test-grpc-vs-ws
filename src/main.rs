use cadence::{BufferedUdpMetricSink, QueuingMetricSink, StatsdClient};
use cadence_macros::set_global_default;
use chrono::Local;
use colored::*;
use dotenv::dotenv;
use env_logger::{self, Builder, Env};
use geyser_stats::{analyze_geyser_stats, GeyserStats};
use grpc_geyser::GrpcGeyser;
use log::{error, info, Level};
use tokio;
use ws_geyser::WsGeyser;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{env, error::Error, io::Write, net::UdpSocket, sync::Arc, time::Duration};
use tokio::sync::Mutex;

use ui::{draw, UiState};

mod error;
mod geyser_stats;
mod grpc_geyser;
mod ui;
mod ws_geyser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Load .env file first
    dotenv().ok();

    // Initialize custom logger
    Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level_color = match record.level() {
                Level::Error => "red",
                Level::Warn => "yellow",
                Level::Info => "green",
                Level::Debug => "blue",
                Level::Trace => "magenta",
            };

            writeln!(
                buf,
                "[{}] {} {}",
                Local::now()
                    .format("%Y-%m-%d %H:%M:%S%.3f")
                    .to_string()
                    .white(),
                record.level().to_string().color(level_color).bold(),
                record.args().to_string().white()
            )
        })
        .init();

    // Setup metrics
    if let Err(e) = setup_metrics() {
        error!("Failed to setup metrics: {}", e);
        return Err(e.into());
    }

    info!("Starting geyser...");

    // preparing environment variables for grpc geyser
    let grpc_endpoint = env::var("GRPC_ENDPOINT").expect("GRPC_ENDPOINT must be set");
    let grpc_x_token = env::var("GRPC_X_TOKEN").expect("GRPC_X_TOKEN must be set");

    // preparing enviroment variables for ws geyser
    let ws_endpoint_with_key =
        env::var("WS_ENDPOINT_WITH_API_KEY").expect("WS_ENDPOINT_WITH_API_KEY must be set");

    // create geyser stats
    let geyser_stats = Arc::new(Mutex::new(GeyserStats::new()));

    // create UI state
    let ui_state = Arc::new(Mutex::new(UiState::new()));
    // create the ws geyser
    WsGeyser::new(ws_endpoint_with_key, geyser_stats.clone());

    // create the grpc geyser
    GrpcGeyser::new(grpc_endpoint, grpc_x_token, geyser_stats.clone());

    tokio::spawn(analyze_geyser_stats(geyser_stats.clone()));

    // Configurar terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Loop principal da UI
    loop {
        terminal.draw(|f| {
            let state = ui_state.try_lock();
            let geyser_stats = geyser_stats.try_lock();

            if let (Ok(mut state), Ok(geyser_stats)) = (state, geyser_stats) {
                // Atualize o estado da UI com os dados mais recentes
                state.update(
                    geyser_stats.last_grpc_slot().unwrap_or(0),
                    geyser_stats.last_ws_slot().unwrap_or(0),
                    geyser_stats
                        .last_slots_time_diff()
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0),
                );

                draw(f, &state);
            } else {
                // Se não conseguir obter os locks, desenhe um estado vazio ou uma mensagem de erro
                draw(f, &UiState::new());
            }
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    break;
                }
            }
        }

        // Adicione um pequeno delay para evitar uso excessivo da CPU
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Restaurar terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn setup_metrics() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_nonblocking(true)?;

    let host = ("127.0.0.1", 8125);
    let udp_sink = BufferedUdpMetricSink::from(host, socket)?;
    let queuing_sing = QueuingMetricSink::from(udp_sink);
    let client = StatsdClient::from_sink("hexis_vs_helius_test", queuing_sing);

    set_global_default(client);
    Ok(())
}
