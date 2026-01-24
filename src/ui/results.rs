//! Search results list view.
//!
//! Renders the file results with virtual scrolling for performance
//! with large result sets.

use eframe::egui::{self, ScrollArea, Sense};

use crate::ipc::protocol::FileResult;

/// View for displaying search results.
pub struct ResultsView;

impl ResultsView {
    /// Display the results list.
    ///
    /// Returns the index of a clicked row, if any.
    pub fn show(ui: &mut egui::Ui, results: &[FileResult], selected: usize) -> Option<usize> {
        let mut clicked_index = None;

        if results.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No results. Start typing to search.");
            });
            return None;
        }

        // Use ScrollArea with show_rows for virtual scrolling
        let row_height = 24.0;
        let available_height = ui.available_height();
        let _visible_rows = (available_height / row_height).ceil() as usize;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, results.len(), |ui, row_range| {
                for i in row_range {
                    if let Some(result) = results.get(i) {
                        let is_selected = i == selected;

                        // Create a selectable row
                        let response = ui.horizontal(|ui| {
                            // Background color for selected row
                            if is_selected {
                                let rect = ui.available_rect_before_wrap();
                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    ui.visuals().selection.bg_fill,
                                );
                            }

                            // Icon based on type
                            let icon = if result.is_dir { "D " } else { "F " };
                            ui.monospace(icon);

                            // Filename (prominent)
                            ui.strong(&result.name);

                            // Spacer
                            ui.add_space(10.0);

                            // Path (dimmed)
                            ui.weak(&result.path);

                            // Right-aligned info
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Modified date
                                ui.weak(format_date(result.modified));
                                ui.add_space(10.0);

                                // Size (only for files)
                                if !result.is_dir {
                                    ui.weak(format_size(result.size));
                                }
                            });
                        });

                        // Make the row clickable
                        if ui.interact(response.response.rect, egui::Id::new(("result", i)), Sense::click()).clicked() {
                            clicked_index = Some(i);
                        }
                    }
                }

                // Note: Scroll-to-selected is handled by egui's scroll area memory
            });

        clicked_index
    }
}

/// Format file size in human-readable format.
///
/// Examples: "1.2 MB", "340 KB", "4.5 GB"
pub fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    const TB: i64 = GB * 1024;

    if bytes < 0 {
        return "---".to_string();
    }

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format Unix timestamp as date string.
///
/// Format: "2024-01-15 14:30"
pub fn format_date(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};

    if timestamp <= 0 {
        return "---".to_string();
    }

    match Local.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        _ => "---".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
        assert_eq!(format_size(1099511627776), "1.0 TB");
        assert_eq!(format_size(-1), "---");
    }

    #[test]
    fn test_format_date() {
        // Test invalid timestamp
        assert_eq!(format_date(0), "---");
        assert_eq!(format_date(-1), "---");

        // Test valid timestamp (2024-01-15 00:00:00 UTC roughly)
        // Exact output depends on timezone, just check it doesn't crash
        let result = format_date(1705276800);
        assert!(!result.is_empty());
        assert_ne!(result, "---");
    }
}
