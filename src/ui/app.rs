//! Main search application window.
//!
//! Implements eframe::App for the search UI with keyboard navigation,
//! search-as-you-type, and file result display.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui;
use tokio::runtime::Handle;

use crate::ipc::IpcClient;
use crate::ipc::protocol::{FileResult, SearchResponse};
use crate::ui::results::ResultsView;
use crate::ui::actions;

/// Debounce duration for search queries (100ms).
const SEARCH_DEBOUNCE_MS: u64 = 100;

/// Maximum results to fetch per query.
const MAX_RESULTS: usize = 100;

/// The main search application.
pub struct SearchApp {
    /// Current search query text.
    query: String,
    /// Search results from last query.
    results: Vec<FileResult>,
    /// Currently selected result index.
    selected_index: usize,
    /// Whether a search is currently pending (debounce).
    search_pending: bool,
    /// Time when the last query change occurred.
    last_query_change: Option<Instant>,
    /// IPC client for service communication (stateless, new connection per search).
    #[allow(dead_code)]
    ipc_client: IpcClient,
    /// Tokio runtime handle for async operations.
    runtime: Handle,
    /// Receiver for hotkey events.
    hotkey_rx: Receiver<()>,
    /// Shared visibility state.
    visible: Arc<AtomicBool>,
    /// Search status message.
    status: String,
    /// Total result count (may exceed results.len()).
    total_count: usize,
    /// Last search duration in milliseconds.
    search_time_ms: u64,
    /// Pending search results (from async task).
    pending_results: Option<std::sync::mpsc::Receiver<SearchResponse>>,
    /// Whether this is the first frame (for initial focus).
    first_frame: bool,
}

impl SearchApp {
    /// Create a new search application.
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        runtime: Handle,
        hotkey_rx: Receiver<()>,
        visible: Arc<AtomicBool>,
    ) -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            search_pending: false,
            last_query_change: None,
            ipc_client: IpcClient::new(),
            runtime,
            hotkey_rx,
            visible,
            status: "Ready".to_string(),
            total_count: 0,
            search_time_ms: 0,
            pending_results: None,
            first_frame: true,
        }
    }

    /// Trigger a search with debouncing.
    fn trigger_search(&mut self) {
        self.search_pending = true;
        self.last_query_change = Some(Instant::now());
    }

    /// Execute the actual search query.
    fn execute_search(&mut self, ctx: &egui::Context) {
        let query = self.query.clone();
        if query.is_empty() {
            self.results.clear();
            self.total_count = 0;
            self.status = "Ready".to_string();
            return;
        }

        self.status = "Searching...".to_string();

        // Create channel for results
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending_results = Some(rx);

        // Clone what we need for the async task
        let ipc_client = IpcClient::new();
        let ctx = ctx.clone();

        // Spawn async search task
        self.runtime.spawn(async move {
            let result = ipc_client.search(&query, MAX_RESULTS).await;
            match result {
                Ok(response) => {
                    let _ = tx.send(response);
                }
                Err(e) => {
                    tracing::error!("Search failed: {}", e);
                    // Send empty response on error
                    let _ = tx.send(SearchResponse {
                        results: Vec::new(),
                        total_count: 0,
                        search_time_ms: 0,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    /// Check for and process pending search results.
    fn check_pending_results(&mut self) {
        if let Some(rx) = &self.pending_results {
            if let Ok(response) = rx.try_recv() {
                self.results = response.results;
                self.total_count = response.total_count;
                self.search_time_ms = response.search_time_ms;
                self.selected_index = 0;
                self.status = format!(
                    "{} results in {}ms",
                    self.total_count, self.search_time_ms
                );
                self.pending_results = None;
            }
        }
    }

    /// Handle keyboard navigation.
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Navigate down
            if i.key_pressed(egui::Key::ArrowDown) {
                if !self.results.is_empty() {
                    self.selected_index = (self.selected_index + 1).min(self.results.len() - 1);
                }
            }

            // Navigate up
            if i.key_pressed(egui::Key::ArrowUp) {
                self.selected_index = self.selected_index.saturating_sub(1);
            }

            // Open selected file
            if i.key_pressed(egui::Key::Enter) {
                if let Some(result) = self.results.get(self.selected_index) {
                    let path = std::path::Path::new(&result.path);
                    if let Err(e) = actions::open_file(path) {
                        tracing::error!("Failed to open file: {}", e);
                        self.status = format!("Failed to open: {}", e);
                    }
                }
            }

            // Close/hide on Escape
            if i.key_pressed(egui::Key::Escape) {
                self.visible.store(false, Ordering::SeqCst);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }

            // Copy path to clipboard (Ctrl+Shift+C)
            if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::C) {
                if let Some(result) = self.results.get(self.selected_index) {
                    let path = std::path::Path::new(&result.path);
                    if let Err(e) = actions::copy_to_clipboard(path) {
                        tracing::error!("Failed to copy path: {}", e);
                        self.status = format!("Failed to copy: {}", e);
                    } else {
                        self.status = "Path copied to clipboard".to_string();
                    }
                }
            }

            // Reveal in Explorer (Ctrl+Shift+E)
            if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::E) {
                if let Some(result) = self.results.get(self.selected_index) {
                    let path = std::path::Path::new(&result.path);
                    if let Err(e) = actions::reveal_in_explorer(path) {
                        tracing::error!("Failed to reveal file: {}", e);
                        self.status = format!("Failed to reveal: {}", e);
                    }
                }
            }
        });
    }

    /// Check for hotkey events.
    fn check_hotkey(&mut self, ctx: &egui::Context) {
        // Non-blocking check for hotkey events
        while self.hotkey_rx.try_recv().is_ok() {
            let is_visible = self.visible.load(Ordering::SeqCst);
            if is_visible {
                self.visible.store(false, Ordering::SeqCst);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            } else {
                self.visible.store(true, Ordering::SeqCst);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
    }
}

impl eframe::App for SearchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for hotkey events
        self.check_hotkey(ctx);

        // Check for pending search results
        self.check_pending_results();

        // Handle keyboard navigation
        self.handle_keyboard(ctx);

        // Check if debounced search should execute
        if self.search_pending {
            if let Some(last_change) = self.last_query_change {
                if last_change.elapsed() >= Duration::from_millis(SEARCH_DEBOUNCE_MS) {
                    self.search_pending = false;
                    self.execute_search(ctx);
                } else {
                    // Request repaint to check again
                    ctx.request_repaint_after(Duration::from_millis(10));
                }
            }
        }

        // Main UI panel
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Search input
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .desired_width(ui.available_width() - 60.0)
                            .hint_text("Type to search files...")
                    );

                    // Request focus on first frame
                    if self.first_frame {
                        response.request_focus();
                        self.first_frame = false;
                    }

                    // Trigger search on text change
                    if response.changed() {
                        self.trigger_search();
                    }
                });

                ui.separator();

                // Results list
                let clicked_index = ResultsView::show(ui, &self.results, self.selected_index);
                if let Some(index) = clicked_index {
                    self.selected_index = index;
                    // Double-click could open the file
                    if let Some(result) = self.results.get(index) {
                        let path = std::path::Path::new(&result.path);
                        if let Err(e) = actions::open_file(path) {
                            tracing::error!("Failed to open file: {}", e);
                        }
                    }
                }

                ui.separator();

                // Status bar
                ui.horizontal(|ui| {
                    ui.label(&self.status);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label("Esc:close  Enter:open  Ctrl+Shift+E:reveal  Ctrl+Shift+C:copy");
                    });
                });
            });
        });
    }
}
