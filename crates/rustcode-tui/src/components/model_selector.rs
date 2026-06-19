//! Model selector dialog — browse providers and models with search/filter.
//!
//! Ported from: `packages/tui/src/component/dialog-model-selector.tsx`
//!
//! The model selector lists providers and their models in a searchable list,
//! showing metadata such as context window, cost per million tokens, and
//! capabilities. Users can select a model to switch the current provider/model.
//!
//! ## Key bindings
//!
//! | Key | Action |
//! |-----|--------|
//! | `Up` / `k` | Previous item |
//! | `Down` / `j` | Next item |
//! | `Enter` | Select model |
//! | `Esc` | Close dialog |
//! | `/` | Focus search bar |
//! | Any printable | Type search query |

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use rustcode_core::provider::{Capabilities, Model, ProviderInfo};
use std::collections::HashMap;

/// Maximum number of model entries to display at once.
const MAX_DISPLAY: usize = 30;

/// A flattened entry in the model list — either a provider header or a model row.
#[derive(Debug, Clone)]
pub enum ModelListEntry {
    /// Provider group header.
    ProviderHeader {
        provider_id: String,
        provider_name: String,
        model_count: usize,
        /// Whether the provider group is expanded.
        expanded: bool,
    },
    /// A single model entry.
    Model {
        provider_id: String,
        provider_name: String,
        model: Model,
        /// Index into the original provider's models map for lookup.
        model_idx: usize,
    },
}

impl ModelListEntry {
    pub fn is_provider_header(&self) -> bool {
        matches!(self, ModelListEntry::ProviderHeader { .. })
    }

    pub fn provider_id(&self) -> &str {
        match self {
            ModelListEntry::ProviderHeader { provider_id, .. } => provider_id,
            ModelListEntry::Model { provider_id, .. } => provider_id,
        }
    }
}

/// State for the model selector dialog.
#[derive(Debug, Default)]
pub struct ModelSelectorState {
    /// Whether the dialog is visible.
    pub visible: bool,
    /// All entries in the flat list (headers + visible models).
    pub entries: Vec<ModelListEntry>,
    /// Currently selected entry index.
    pub selected: usize,
    /// Search/filter query string.
    pub query: String,
    /// Whether the search bar is focused.
    pub search_focused: bool,
    /// Currently active provider ID.
    pub current_provider: Option<String>,
    /// Currently active model ID.
    pub current_model: Option<String>,
    /// Provider details (ID -> ProviderInfo).
    providers: HashMap<String, ProviderInfo>,
}

impl ModelSelectorState {
    pub fn new() -> Self {
        Self {
            visible: false,
            entries: Vec::new(),
            selected: 0,
            query: String::new(),
            search_focused: true,
            current_provider: None,
            current_model: None,
            providers: HashMap::new(),
        }
    }

    /// Show the dialog with provider data.
    pub fn show(
        &mut self,
        providers: HashMap<String, ProviderInfo>,
        current_provider: Option<String>,
        current_model: Option<String>,
    ) {
        self.providers = providers;
        self.current_provider = current_provider;
        self.current_model = current_model;
        self.visible = true;
        self.selected = 0;
        self.query.clear();
        self.search_focused = true;
        self.rebuild_entries();
    }

    /// Hide the dialog.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Rebuild the flat entry list from providers.
    fn rebuild_entries(&mut self) {
        self.entries.clear();
        let query_lower = self.query.to_lowercase();

        // Sort providers: connected first, then alphabetically
        let mut provider_ids: Vec<String> = self.providers.keys().cloned().collect();
        provider_ids.sort_by(|a, b| {
            let a_current = self.current_provider.as_deref() == Some(a);
            let b_current = self.current_provider.as_deref() == Some(b);
            if a_current && !b_current {
                std::cmp::Ordering::Less
            } else if !a_current && b_current {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });

        for pid in &provider_ids {
            if let Some(provider) = self.providers.get(pid) {
                // Collect matching models
                let mut model_ids: Vec<String> = provider.models.keys().cloned().collect();
                model_ids.sort();

                let matching_models: Vec<(usize, &String)> = model_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, mid)| {
                        if query_lower.is_empty() {
                            return true;
                        }
                        mid.to_lowercase().contains(&query_lower)
                            || pid.to_lowercase().contains(&query_lower)
                            || provider
                                .models
                                .get(*mid)
                                .map(|m| {
                                    m.name.to_lowercase().contains(&query_lower)
                                        || m.family
                                            .as_ref()
                                            .map(|f| f.to_lowercase().contains(&query_lower))
                                            .unwrap_or(false)
                                })
                                .unwrap_or(false)
                    })
                    .collect();

                if matching_models.is_empty() && !query_lower.is_empty() {
                    // If there's a query and no models match, also skip the header
                    continue;
                }

                // Add header
                let header = ModelListEntry::ProviderHeader {
                    provider_id: pid.clone(),
                    provider_name: provider.name.clone(),
                    model_count: matching_models.len(),
                    expanded: true,
                };
                self.entries.push(header);

                // Add models
                for (orig_idx, mid) in &matching_models {
                    if let Some(model) = provider.models.get(*mid) {
                        let entry = ModelListEntry::Model {
                            provider_id: pid.clone(),
                            provider_name: provider.name.clone(),
                            model: model.clone(),
                            model_idx: *orig_idx,
                        };
                        self.entries.push(entry);
                    }
                }
            }
        }

        if self.entries.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    /// Select the next entry.
    pub fn next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
        }
    }

    /// Select the previous entry.
    pub fn prev(&mut self) {
        if !self.entries.is_empty() {
            self.selected = if self.selected == 0 {
                self.entries.len().saturating_sub(1)
            } else {
                self.selected - 1
            };
        }
    }

    /// Get the selected model, if the selected entry is a model row.
    pub fn selected_model(&self) -> Option<(String, String, Model)> {
        match self.entries.get(self.selected) {
            Some(ModelListEntry::Model {
                provider_id, model, ..
            }) => Some((provider_id.clone(), model.id.clone(), model.clone())),
            _ => None,
        }
    }

    /// Format context window size for display.
    fn format_context_size(context: u64) -> String {
        if context >= 1_000_000_000 {
            format!("{}B", context / 1_000_000_000)
        } else if context >= 1_000_000 {
            format!("{}M", context / 1_000_000)
        } else if context >= 1_000 {
            format!("{}K", context / 1_000)
        } else {
            format!("{}", context)
        }
    }

    /// Format cost per million tokens for display.
    fn format_cost(cost: &rustcode_core::provider::Cost) -> String {
        format!("${:.2}/M in, ${:.2}/M out", cost.input, cost.output)
    }

    /// Build capability tags as a string.
    fn capability_tags(caps: &Capabilities) -> String {
        let mut tags = Vec::new();
        if caps.reasoning {
            tags.push("reasoning");
        }
        if caps.toolcall {
            tags.push("tools");
        }
        if caps.attachment {
            tags.push("files");
        }
        {
            let has_interleaved = match &caps.interleaved {
                rustcode_core::provider::InterleavedSupport::Bool(b) => *b,
                rustcode_core::provider::InterleavedSupport::Field { .. } => true,
            };
            if has_interleaved {
                tags.push("interleaved");
            }
        }
        if tags.is_empty() {
            "basic".to_string()
        } else {
            tags.join(", ")
        }
    }

    /// Handle a key event. Returns the action to take.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<ModelSelectorAction> {
        if !self.visible {
            return None;
        }

        match key {
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.hide();
                Some(ModelSelectorAction::Close)
            }

            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.prev();
                Some(ModelSelectorAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Down, ..
            }
            | KeyEvent {
                code: KeyCode::Char('j'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.next();
                Some(ModelSelectorAction::Navigate)
            }

            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                // If on a provider header, toggle expand; if on a model, select it
                if let Some(entry) = self.entries.get(self.selected) {
                    if entry.is_provider_header() {
                        // In full impl we'd toggle expand. For now, select is a no-op on headers.
                        Some(ModelSelectorAction::Navigate)
                    } else {
                        let selected = self.selected_model();
                        if let Some((pid, mid, _model)) = selected {
                            self.hide();
                            Some(ModelSelectorAction::Select {
                                provider_id: pid,
                                model_id: mid,
                            })
                        } else {
                            Some(ModelSelectorAction::Navigate)
                        }
                    }
                } else {
                    Some(ModelSelectorAction::Navigate)
                }
            }

            // Toggle search focus
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.search_focused = true;
                self.query.clear();
                self.rebuild_entries();
                Some(ModelSelectorAction::Navigate)
            }

            // Backspace in search
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if self.search_focused {
                    self.query.pop();
                    self.rebuild_entries();
                    self.selected = 0;
                }
                Some(ModelSelectorAction::Navigate)
            }

            // Printable characters in search
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } if self.search_focused => {
                self.query.push(ch);
                self.rebuild_entries();
                self.selected = 0;
                Some(ModelSelectorAction::Navigate)
            }

            _ => Some(ModelSelectorAction::Navigate),
        }
    }
}

/// Actions returned by the model selector key handler.
#[derive(Debug, Clone)]
pub enum ModelSelectorAction {
    /// Close the dialog.
    Close,
    /// Navigation occurred (redraw needed).
    Navigate,
    /// Select a model.
    Select {
        provider_id: String,
        model_id: String,
    },
}

/// Render the model selector as a modal dialog.
pub fn render_model_selector(f: &mut Frame, area: Rect, state: &ModelSelectorState) {
    if !state.visible {
        return;
    }

    let dialog_width = (area.width as f64 * 0.6).min(90.0).max(50.0) as u16;
    let dialog_height = (area.height as f64 * 0.7).min(35.0).max(18.0) as u16;
    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 4;

    let dialog_area = Rect::new(area.x + dialog_x, area.y + dialog_y, dialog_width, dialog_height);

    f.render_widget(Clear, dialog_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Select Model ")
        .title_bottom(" j/k:nav  Enter:select  /:search  Esc:close ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(dialog_area);
    f.render_widget(block, dialog_area);

    // ── Layout ─────────────────────────────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Search bar
            Constraint::Min(3),     // List + metadata
        ])
        .split(inner);

    // ── Search bar ─────────────────────────────────────────────
    let search_style = if state.search_focused {
        Style::default().fg(Color::Yellow).bg(Color::Black)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let search_text = if state.query.is_empty() {
        if state.search_focused {
            Span::styled(
                " Type to filter models... ",
                search_style.add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" Press / to search... ", search_style)
        }
    } else {
        Span::styled(format!(" /{} ", state.query), search_style)
    };

    let search_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(
            if state.search_focused {
                Color::Yellow
            } else {
                Color::DarkGray
            },
        ));
    let search_inner = search_block.inner(chunks[0]);
    f.render_widget(search_block, chunks[0]);
    f.render_widget(Paragraph::new(Line::from(search_text)), search_inner);

    // ── List + metadata ────────────────────────────────────────
    let list_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(chunks[1]);

    // ── Left: Model list ───────────────────────────────────────
    if state.entries.is_empty() {
        f.render_widget(
            Paragraph::new("No models available. Connect a provider first.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true }),
            list_cols[0],
        );
    } else {
        let display_entries: Vec<(usize, &ModelListEntry)> = state
            .entries
            .iter()
            .enumerate()
            .take(MAX_DISPLAY)
            .collect();

        let items: Vec<ListItem> = display_entries
            .iter()
            .map(|(i, entry)| {
                let is_selected = *i == state.selected;

                match entry {
                    ModelListEntry::ProviderHeader {
                        provider_id,
                        provider_name,
                        model_count,
                        ..
                    } => {
                        let is_current = state.current_provider.as_deref() == Some(provider_id.as_str());
                        let line = Line::from(vec![
                            Span::styled(
                                if is_selected { " ▶ " } else { "   " },
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::styled(
                                if is_current { "* " } else { "  " },
                                Style::default().fg(Color::Green),
                            ),
                            Span::styled(
                                provider_name,
                                Style::default()
                                    .fg(if is_selected { Color::Black } else { Color::Cyan })
                                    .add_modifier(Modifier::BOLD)
                                    .bg(if is_selected { Color::Yellow } else { Color::Reset }),
                            ),
                            Span::raw("  "),
                            Span::styled(
                                format!("({} models)", model_count),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]);
                        ListItem::new(line)
                    }
                    ModelListEntry::Model {
                        provider_id,
                        model,
                        ..
                    } => {
                        let is_current = state.current_provider.as_deref() == Some(provider_id.as_str())
                            && state.current_model.as_deref() == Some(model.id.as_str());

                        let row_style = if is_selected {
                            Style::default().fg(Color::Black).bg(Color::Cyan)
                        } else {
                            Style::default().fg(Color::White)
                        };

                        let active_marker = if is_current {
                            Span::styled("* ", Style::default().fg(Color::Green))
                        } else {
                            Span::raw("  ")
                        };

                        let name = &model.name;
                        let context = Self::format_context_size(model.limit.context);

                        let line = Line::from(vec![
                            Span::raw("     "),
                            active_marker,
                            Span::styled(
                                name,
                                if is_selected {
                                    row_style.add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                                },
                            ),
                            Span::raw("  "),
                            Span::styled(
                                context,
                                if is_selected {
                                    Style::default().fg(Color::Black)
                                } else {
                                    Style::default().fg(Color::DarkGray)
                                },
                            ),
                            Span::raw("  "),
                            Span::styled(
                                Self::capability_tags(&model.capabilities),
                                if is_selected {
                                    Style::default().fg(Color::Black)
                                } else {
                                    Style::default().fg(Color::Gray)
                                },
                            ),
                        ]);

                        ListItem::new(line)
                    }
                }
            })
            .collect();

        let list = List::new(items);
        f.render_widget(list, list_cols[0]);
    }

    // ── Right: Model details ───────────────────────────────────
    let detail_block = Block::default()
        .borders(Borders::LEFT)
        .title(" Model Details ")
        .border_style(Style::default().fg(Color::Blue));

    let detail_inner = detail_block.inner(list_cols[1]);
    f.render_widget(detail_block, list_cols[1]);

    match state.entries.get(state.selected) {
        Some(ModelListEntry::Model { model, provider_id, .. }) => {
            let mut lines: Vec<Line> = Vec::new();

            lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::Gray)),
                Span::styled(&model.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ]));

            lines.push(Line::from(vec![
                Span::styled("Provider: ", Style::default().fg(Color::Gray)),
                Span::styled(provider_id, Style::default().fg(Color::Cyan)),
            ]));

            if let Some(ref family) = model.family {
                lines.push(Line::from(vec![
                    Span::styled("Family: ", Style::default().fg(Color::Gray)),
                    Span::styled(family, Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(vec![
                Span::styled("Context: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{} tokens", model.limit.context),
                    Style::default().fg(Color::White),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled("Cost: ", Style::default().fg(Color::Gray)),
                Span::styled(Self::format_cost(&model.cost), Style::default().fg(Color::Yellow)),
            ]));

            lines.push(Line::from(""));

            lines.push(Line::from(vec![
                Span::styled("Capabilities:", Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
            ]));

            lines.push(Line::from(vec![
                Span::raw("  Reasoning: "),
                Span::styled(
                    if model.capabilities.reasoning { "yes" } else { "no" },
                    if model.capabilities.reasoning {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
            ]));

            lines.push(Line::from(vec![
                Span::raw("  Tool calls: "),
                Span::styled(
                    if model.capabilities.toolcall { "yes" } else { "no" },
                    if model.capabilities.toolcall {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
            ]));

            lines.push(Line::from(vec![
                Span::raw("  Attachments: "),
                Span::styled(
                    if model.capabilities.attachment { "yes" } else { "no" },
                    if model.capabilities.attachment {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Red)
                    },
                ),
            ]));

            lines.push(Line::from(vec![
                Span::raw("  Input mods: "),
                Span::styled(
                    format!("text:{} img:{}",
                        model.capabilities.input.text,
                        model.capabilities.input.image),
                    Style::default().fg(Color::White),
                ),
            ]));

            if !model.release_date.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("Released: ", Style::default().fg(Color::Gray)),
                    Span::styled(&model.release_date, Style::default().fg(Color::DarkGray)),
                ]));
            }

            let text = Text::from(lines);
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), detail_inner);
        }
        Some(ModelListEntry::ProviderHeader { provider_name, model_count, .. }) => {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(vec![
                Span::styled(provider_name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} model(s) available", model_count),
                    Style::default().fg(Color::Gray),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Use Enter to select a model below.", Style::default().fg(Color::DarkGray)),
            ]));

            let text = Text::from(lines);
            f.render_widget(Paragraph::new(text).wrap(Wrap { trim: true }), detail_inner);
        }
        None => {
            f.render_widget(
                Paragraph::new("No model selected.").style(Style::default().fg(Color::DarkGray)),
                detail_inner,
            );
        }
    }
}
