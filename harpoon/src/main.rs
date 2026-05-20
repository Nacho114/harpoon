pub(crate) mod persistence;

use core::fmt;
use persistence::Persistence;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use zellij_tile::prelude::*;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A tracked pane, combining zellij's PaneInfo with its parent TabInfo.
///
/// Per the zellij API docs, `PaneInfo.id` combined with `PaneInfo.is_plugin`
/// uniquely identifies a pane across the entire session. Since harpoon only
/// tracks terminal panes (!is_plugin), `pane_info.id` alone is a stable,
/// globally unique identifier.
///
/// Docs: https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PaneInfo.html
#[derive(Clone, Serialize, Deserialize)]
pub struct Pane {
    pub pane_info: PaneInfo,
    pub tab_info: TabInfo,
    #[serde(default)]
    pub last_accessed: u64,
}

impl fmt::Display for Pane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} | {}", self.tab_info.name, self.pane_info.title)
    }
}

//<--------- TODO: Replace with official functions once available

/// Returns the currently active tab, if any.
///
/// `TabInfo.active` is set by zellij on the tab the user is currently viewing.
/// Docs: https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.TabInfo.html
fn get_focused_tab(tab_infos: &Vec<TabInfo>) -> Option<TabInfo> {
    tab_infos.iter().find(|t| t.active).cloned()
}

/// Returns the focused terminal pane in the given tab.
///
/// `PaneManifest.panes` is a HashMap keyed by tab position (0-indexed), containing
/// all panes in that tab including tiled, floating, and suppressed panes.
///
/// When harpoon itself has focus (it's a plugin pane), no terminal pane will have
/// `is_focused = true`, so we fall back to the first non-plugin pane in the tab.
///
/// Docs: https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PaneManifest.html
fn get_focused_pane(tab_position: usize, pane_manifest: &PaneManifest) -> Option<PaneInfo> {
    let panes = pane_manifest.panes.get(&tab_position)?;
    // First, try to find a focused non-plugin pane
    if let Some(pane) = panes.iter().find(|p| p.is_focused && !p.is_plugin) {
        return Some(pane.clone());
    }
    // Fallback: if no focused non-plugin pane (e.g. harpoon itself has focus),
    // return the first non-plugin pane in the tab
    panes.iter().find(|p| !p.is_plugin).cloned()
}

//--------->

// ----------------------------------- Update ------------------------------------------------

/// Filters the stored pane list, removing any panes that no longer exist and
/// updating tab info for panes whose tab was moved/reordered.
///
/// `PaneInfo.id` is unique per session when combined with `is_plugin`. Since we
/// only track terminal panes (!is_plugin), `id` alone is sufficient to identify
/// a pane across tab position changes.
///
/// Docs: https://docs.rs/zellij-tile/latest/zellij_tile/prelude/struct.PaneInfo.html
fn get_valid_panes(
    panes: &Vec<Pane>,
    pane_manifest: &PaneManifest,
    tab_infos: &Vec<TabInfo>,
) -> Vec<Pane> {
    let mut new_panes: Vec<Pane> = Vec::default();
    for pane in panes {
        // Search all tabs for this pane by its session-unique ID.
        // Tab positions can change when tabs are created, deleted, or moved,
        // so we search the full manifest rather than relying on the stored position.
        for (tab_position, tab_panes) in &pane_manifest.panes {
            if let Some(pane_info) = tab_panes
                .iter()
                .find(|p| !p.is_plugin && p.id == pane.pane_info.id)
            {
                if let Some(tab_info) = tab_infos.iter().find(|t| t.position == *tab_position) {
                    new_panes.push(Pane {
                        pane_info: pane_info.clone(),
                        tab_info: tab_info.clone(),
                        last_accessed: pane.last_accessed,
                    });
                    break;
                }
            }
        }
    }
    new_panes
}

#[derive(Default, Serialize, Deserialize)]
struct TimestampMap {
    entries: Vec<TimestampEntry>,
}

#[derive(Clone, Serialize, Deserialize)]
struct TimestampEntry {
    tab_name: String,
    pane_title: String,
    #[serde(default)]
    pane_id: u32,
    last_accessed: u64,
}

#[derive(Default)]
struct State {
    selected: usize,
    panes: Vec<Pane>,
    focused_pane: Option<Pane>,
    tab_info: Option<Vec<TabInfo>>,
    pane_manifest: Option<PaneManifest>,
    session_name: Option<String>,
    persistence: Persistence,
    recent_sort: bool,
    worker_timestamps: TimestampMap,
    timestamps_loaded: bool,
    search_query: String,
    search_mode: bool,
}

impl State {

    fn timestamps_file_path(session_name: &str) -> String {
        format!("${{XDG_DATA_HOME:-$HOME/.local/share}}/zellij-harpoon/{}-timestamps.json", session_name)
    }

    fn load_worker_timestamps(&mut self) {
        let Some(session) = &self.session_name else { return };
        self.timestamps_loaded = false;
        let file_path = Self::timestamps_file_path(session);
        let cmd = format!("cat {} 2>/dev/null || echo '{{\"entries\":[]}}'", file_path);
        let mut ctx = BTreeMap::new();
        ctx.insert("source".to_string(), "timestamps".to_string());
        run_command(&["sh", "-c", &cmd], ctx);
    }

    /// Returns indices into self.panes that should be displayed.
    fn display_indices(&self) -> Vec<usize> {
        let query = self.search_query.to_lowercase();
        self.panes
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                if self.recent_sort {
                    if let Some(f) = &self.focused_pane {
                        if f.pane_info.id == p.pane_info.id {
                            return false;
                        }
                    }
                }
                if !query.is_empty() {
                    let haystack = format!("{} | {}", p.tab_info.name, p.pane_info.title).to_lowercase();
                    if !fuzzy_match(&query, &haystack) {
                        return false;
                    }
                }
                true
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn display_len(&self) -> usize {
        self.display_indices().len()
    }

    fn clamp_selected(&mut self) {
        let len = self.display_len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn select_down(&mut self) {
        let len = self.display_len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    fn select_up(&mut self) {
        let len = self.display_len();
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
            return;
        }
        self.selected -= 1;
    }

    /// Maps the display-list selected index to the actual index in self.panes.
    fn selected_pane_index(&self) -> Option<usize> {
        self.display_indices().get(self.selected).copied()
    }

    fn sort_panes(&mut self) {
        if self.recent_sort {
            self.panes.sort_by(|x, y| y.last_accessed.cmp(&x.last_accessed));
        } else {
            self.panes.sort_by(|x, y| x.tab_info.position.cmp(&y.tab_info.position));
        }
    }

    /// Reconciles the stored pane list against the latest manifest and updates
    /// the currently focused pane. Called on every TabUpdate and PaneUpdate event.
    fn update_panes(&mut self) -> Option<()> {
        let pane_manifest = self.pane_manifest.clone()?;
        let tab_info = self.tab_info.clone()?;

        // Drop any panes that no longer exist and refresh tab info for moved ones
        self.panes = get_valid_panes(&self.panes.clone(), &pane_manifest, &tab_info);

        // Match pending bookmarks to live panes (restores panes after session reload)
        let new_panes =
            self.persistence
                .match_pending_bookmarks(&self.panes, &pane_manifest, &tab_info);
        if !new_panes.is_empty() {
            self.panes.extend(new_panes);
            self.sort_panes();
        }

        // Track which pane the user was in before harpoon opened
        let focused_tab = get_focused_tab(&tab_info)?;
        let focused_pane_info = get_focused_pane(focused_tab.position, &pane_manifest)?;
        self.focused_pane = Some(Pane {
            pane_info: focused_pane_info,
            tab_info: focused_tab.clone(),
            last_accessed: 0,
        });

        // In recent_sort mode, merge worker timestamps into panes and auto-add all panes
        if self.recent_sort && self.timestamps_loaded {
            // Auto-add all terminal panes from manifest
            let current_ids: Vec<u32> = self.panes.iter().map(|p| p.pane_info.id).collect();
            for (tab_position, manifest_panes) in &pane_manifest.panes {
                if let Some(tab) = tab_info.iter().find(|t| t.position == *tab_position) {
                    for pane in manifest_panes {
                        if !pane.is_plugin && !current_ids.contains(&pane.id) {
                            let ts = self.worker_timestamps.entries.iter()
                                .find(|e| e.pane_id == pane.id)
                                .or_else(|| self.worker_timestamps.entries.iter().find(|e| e.pane_id == 0 && e.tab_name == tab.name && e.pane_title == pane.title))
                                .map(|e| e.last_accessed)
                                .unwrap_or(0);
                            if ts > 0 {
                                self.panes.push(Pane {
                                    pane_info: pane.clone(),
                                    tab_info: tab.clone(),
                                    last_accessed: ts,
                                });
                            }
                        }
                    }
                }
            }
            // Update timestamps for existing panes from worker data
            for pane in self.panes.iter_mut() {
                let ts = self.worker_timestamps.entries.iter()
                    .find(|e| e.pane_id == pane.pane_info.id)
                    .or_else(|| self.worker_timestamps.entries.iter().find(|e| e.pane_id == 0 && e.tab_name == pane.tab_info.name && e.pane_title == pane.pane_info.title))
                    .map(|e| e.last_accessed)
                    .unwrap_or(0);
                if ts > pane.last_accessed {
                    pane.last_accessed = ts;
                }
            }
            self.sort_panes();
        }

        // In recent_sort mode, always start at 0 (most recent, excluding focused).
        // Otherwise, move cursor to the focused pane if it's in the list.
        if self.recent_sort {
            self.selected = 0;
        } else {
            if let Some(focused) = &self.focused_pane {
                let display = self.display_indices();
                if let Some(display_idx) = display.iter().position(|&i| {
                    self.panes[i].pane_info.id == focused.pane_info.id
                }) {
                    self.selected = display_idx;
                }
            }
        }
        self.clamp_selected();

        if self.persistence.has_changed(&self.panes) {
            self.persistence
                .save_to_disk(&self.session_name, &self.panes);
        }

        Some(())
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.recent_sort = configuration
            .get("recent_sort")
            .map(|v| v == "true")
            .unwrap_or(false);
        request_permission(&[
            PermissionType::RunCommands,
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[
            EventType::Key,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::PermissionRequestResult,
            EventType::SessionUpdate,
            EventType::RunCommandResult,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::TabUpdate(tab_info) => {
                self.tab_info = Some(tab_info);
                if self.recent_sort && self.session_name.is_some() {
                    self.load_worker_timestamps();
                }
                self.update_panes();
                should_render = true;
            }
            Event::PaneUpdate(pane_manifest) => {
                self.pane_manifest = Some(pane_manifest);
                if self.recent_sort && self.session_name.is_some() {
                    self.load_worker_timestamps();
                }
                self.update_panes();
                should_render = true;
            }
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                // Rename the pane after permissions are granted, since
                // rename_plugin_pane requires ChangeApplicationState permission.
                let plugin_ids = get_plugin_ids();
                rename_plugin_pane(plugin_ids.plugin_id, "harpoon");
            }
            Event::SessionUpdate(session_infos, _) => {
                if self.session_name.is_none() {
                    if let Some(current) = session_infos.iter().find(|s| s.is_current_session) {
                        self.session_name = Some(current.name.clone());
                        self.persistence.load_from_disk(&self.session_name);
                        if self.recent_sort {
                            self.load_worker_timestamps();
                        }
                    }
                }
            }
            Event::RunCommandResult(_exit_code, stdout, _stderr, context) => {
                let source = context.get("source").map(|s| s.as_str());
                if source == Some("load") {
                    let content = String::from_utf8_lossy(&stdout);
                    match self.persistence.on_load_command(&content) {
                        Ok(_) => {
                            self.update_panes();
                            should_render = true;
                        }
                        Err(e) => {
                            eprintln!("{e}");
                        }
                    }
                } else if source == Some("timestamps") {
                    let content = String::from_utf8_lossy(&stdout);
                    if let Ok(map) = serde_json::from_str::<TimestampMap>(&content) {
                        self.worker_timestamps = map;
                    }
                    self.timestamps_loaded = true;
                    self.update_panes();
                    should_render = true;
                }
            }
            Event::Key(key) => if self.search_mode {
                match key.bare_key {
                    BareKey::Esc => {
                        self.search_mode = false;
                        self.search_query.clear();
                        self.selected = 0;
                        self.clamp_selected();
                        should_render = true;
                    }
                    BareKey::Backspace => {
                        self.search_query.pop();
                        if self.search_query.is_empty() {
                            self.search_mode = false;
                        }
                        self.selected = 0;
                        self.clamp_selected();
                        should_render = true;
                    }
                    BareKey::Enter => {
                        if let Some(idx) = self.selected_pane_index() {
                            self.panes[idx].last_accessed = now_secs();
                            let pane_id = self.panes[idx].pane_info.id;
                            self.persistence
                                .save_to_disk(&self.session_name, &self.panes);
                            self.search_mode = false;
                            self.search_query.clear();
                            hide_self();
                            focus_terminal_pane(pane_id, true);
                        }
                    }
                    BareKey::Down => {
                        if self.display_len() > 0 {
                            self.select_down();
                            should_render = true;
                        }
                    }
                    BareKey::Up => {
                        if self.display_len() > 0 {
                            self.select_up();
                            should_render = true;
                        }
                    }
                    BareKey::Char(c) => {
                        self.search_query.push(c);
                        self.selected = 0;
                        self.clamp_selected();
                        should_render = true;
                    }
                    _ => (),
                }
            } else {
                match key.bare_key {
                    BareKey::Char('/') => {
                        self.search_mode = true;
                        should_render = true;
                    }
                    BareKey::Char('A') => {
                        let current_ids: Vec<u32> = self.panes.iter().map(|p| p.pane_info.id).collect();
                        if let Some(pane_manifest) = &self.pane_manifest {
                            if let Some(tab_info) = &self.tab_info {
                                for (tab_position, panes) in &pane_manifest.panes {
                                    if let Some(tab) = tab_info.iter().find(|t| t.position == *tab_position) {
                                        for pane in panes {
                                            if !pane.is_plugin && !current_ids.contains(&pane.id) {
                                                self.panes.push(Pane {
                                                    pane_info: pane.clone(),
                                                    tab_info: tab.clone(),
                                                    last_accessed: 0,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        self.sort_panes();
                        self.persistence
                            .save_to_disk(&self.session_name, &self.panes);
                        should_render = true;
                        hide_self();
                    }
                    BareKey::Char('a') => {
                        if let Some(pane) = &self.focused_pane {
                            if !self.panes.iter().any(|p| p.pane_info.id == pane.pane_info.id) {
                                let mut new_pane = pane.clone();
                                new_pane.last_accessed = now_secs();
                                self.panes.push(new_pane);
                                self.sort_panes();
                                self.persistence
                                    .save_to_disk(&self.session_name, &self.panes);
                            }
                        }
                        should_render = true;
                        hide_self();
                    }
                    BareKey::Char('d') => {
                        if let Some(idx) = self.selected_pane_index() {
                            self.panes.remove(idx);
                            self.persistence
                                .save_to_disk(&self.session_name, &self.panes);
                        }
                        self.clamp_selected();
                        should_render = true;
                    }
                    BareKey::Char('c') | BareKey::Esc => {
                        hide_self();
                    }
                    BareKey::Down | BareKey::Char('j') => {
                        if self.display_len() > 0 {
                            self.select_down();
                            should_render = true;
                        }
                    }
                    BareKey::Up | BareKey::Char('k') => {
                        if self.display_len() > 0 {
                            self.select_up();
                            should_render = true;
                        }
                    }
                    BareKey::Enter | BareKey::Char('l') => {
                        if let Some(idx) = self.selected_pane_index() {
                            self.panes[idx].last_accessed = now_secs();
                            let pane_id = self.panes[idx].pane_info.id;
                            self.persistence
                                .save_to_disk(&self.session_name, &self.panes);
                            hide_self();
                            focus_terminal_pane(pane_id, true);
                        }
                    }
                    _ => (),
                }
            },
            _ => (),
        };

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let display = self.display_indices();
        let mut y = 0;

        if self.search_mode {
            let search_line = format!("/{}", self.search_query);
            print_text_with_coordinates(Text::new(&search_line), 0, y, None, None);
        } else {
            let header = format!("==== {} panes ====", display.len());
            let x = cols.saturating_sub(header.len()) / 2;
            print_text_with_coordinates(Text::new(&header), x, y, None, None);
        }
        y += 1;

        for (display_idx, &pane_idx) in display.iter().enumerate() {
            let pane = &self.panes[pane_idx];
            let pane_str = pane.to_string();
            let mut text = if display_idx == self.selected {
                Text::new(&pane_str).selected()
            } else {
                Text::new(&pane_str)
            };
            if self.search_mode && !self.search_query.is_empty() {
                for range in fuzzy_match_indices(&self.search_query, &pane_str) {
                    text = text.color_range(3, range);
                }
            }
            print_text_with_coordinates(text, 0, y, None, None);
            y += 1;
        }

        let hint_y = rows.saturating_sub(1);
        let hint_line = build_hint_line(cols);
        print_text_with_coordinates(hint_line, 0, hint_y, None, None);
    }
}

/// Fuzzy match: all characters in needle must appear in haystack in order.
fn fuzzy_match(needle: &str, haystack: &str) -> bool {
    let mut haystack_chars = haystack.chars();
    for nc in needle.chars() {
        if haystack_chars.find(|&hc| hc == nc).is_none() {
            return false;
        }
    }
    true
}

/// Returns byte-offset ranges of matched characters for highlighting.
fn fuzzy_match_indices(needle: &str, haystack: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let needle_lower = needle.to_lowercase();
    let haystack_lower = haystack.to_lowercase();
    let mut h_iter = haystack_lower.char_indices().peekable();
    for nc in needle_lower.chars() {
        while let Some((byte_pos, hc)) = h_iter.next() {
            if hc == nc {
                ranges.push(byte_pos..byte_pos + hc.len_utf8());
                break;
            }
        }
    }
    ranges
}

fn build_hint_line(cols: usize) -> Text {
    let (line, key_ranges) = if cols > 75 {
        build_wide_hints()
    } else if cols > 50 {
        build_medium_hints()
    } else {
        build_narrow_hints()
    };

    let mut text = Text::new(&line);
    for range in key_ranges {
        text = text.color_range(3, range);
    }
    text
}

fn build_wide_hints() -> (String, Vec<std::ops::Range<usize>>) {
    let parts = [
        ("<a>", " add pane"),
        ("<A>", " add all"),
        ("<d>", " delete"),
        ("<j/k>", " navigate"),
        ("<Enter>", " focus"),
        ("<Esc>", " close"),
    ];
    build_hint_string(&parts, ", ")
}

fn build_medium_hints() -> (String, Vec<std::ops::Range<usize>>) {
    let parts = [
        ("<a>", " add"),
        ("<A>", " all"),
        ("<d>", " del"),
        ("<j/k>", " nav"),
        ("<Enter>", " go"),
        ("<Esc>", " quit"),
    ];
    build_hint_string(&parts, ", ")
}

fn build_narrow_hints() -> (String, Vec<std::ops::Range<usize>>) {
    let parts = [
        ("<a>", " add"),
        ("<d>", " del"),
        ("<Enter>", " go"),
        ("<Esc>", ""),
    ];
    build_hint_string(&parts, " ")
}

fn build_hint_string(
    parts: &[(&str, &str)],
    separator: &str,
) -> (String, Vec<std::ops::Range<usize>>) {
    let mut result = String::new();
    let mut key_ranges = Vec::new();

    for (i, (key, desc)) in parts.iter().enumerate() {
        if i > 0 {
            result.push_str(separator);
        }
        let start = result.len();
        result.push_str(key);
        let end = result.len();
        key_ranges.push(start..end);
        result.push_str(desc);
    }

    (result, key_ranges)
}
