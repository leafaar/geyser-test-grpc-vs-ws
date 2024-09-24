use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    symbols,
    widgets::{Axis, Block, Borders, Chart, Dataset, Paragraph},
    Frame,
};

pub struct UiState {
    pub grpc_slot: u64,
    pub ws_slot: u64,
    pub diff_ms: i64,
    pub diff_blocks: i64,
    pub history: Vec<(f64, f64)>,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            grpc_slot: 0,
            ws_slot: 0,
            diff_ms: 0,
            diff_blocks: 0,
            history: Vec::new(),
        }
    }

    pub fn update(&mut self, grpc_slot: u64, ws_slot: u64, diff_ms: i64) {
        self.grpc_slot = grpc_slot;
        self.ws_slot = ws_slot;
        self.diff_ms = diff_ms;
        self.diff_blocks = (grpc_slot as i64) - (ws_slot as i64);

        let time = self.history.len() as f64;
        self.history.push((time, diff_ms as f64));
        if self.history.len() > 100 {
            self.history.remove(0);
        }
    }
}

pub fn draw(f: &mut Frame, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(f.area());

    let grpc_paragraph = Paragraph::new(format!("gRPC Slot: {}", state.grpc_slot))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL).title("gRPC"));
    f.render_widget(grpc_paragraph, chunks[0]);

    let ws_paragraph = Paragraph::new(format!("WebSocket Slot: {}", state.ws_slot))
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL).title("WebSocket"));
    f.render_widget(ws_paragraph, chunks[1]);

    let diff_paragraph = Paragraph::new(format!(
        "Difference: {}ms | {} blocks",
        state.diff_ms, state.diff_blocks
    ))
    .style(Style::default().fg(Color::Yellow))
    .block(Block::default().borders(Borders::ALL).title("Difference"));
    f.render_widget(diff_paragraph, chunks[2]);

    let dataset = Dataset::default()
        .name("Difference (ms)")
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .data(&state.history);

    let chart = Chart::new(vec![dataset])
        .block(Block::default().title("History").borders(Borders::ALL))
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 100.0]),
        )
        .y_axis(
            Axis::default()
                .title("Difference (ms)")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 1000.0]),
        );

    f.render_widget(chart, chunks[3]);
}
