pub(crate) mod persistence;

use core::fmt;
use persistence::Persistence;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use zellij_tile::prelude::*;

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
                    });
                    break;
                }
            }
        }
    }
    new_panes
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
}

impl State {
    fn clamp_selected(&mut self) {
        if self.panes.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.panes.len() {
            self.selected = self.panes.len() - 1;
        }
    }

    fn select_down(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.panes.len();
    }

    fn select_up(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.panes.len() - 1;
            return;
        }
        self.selected -= 1;
    }

    fn sort_panes(&mut self) {
        self.panes.sort_by(|x, y| x.tab_info.position.cmp(&y.tab_info.position));
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
            tab_info: focused_tab,
        });

        // Move cursor to the focused pane if it's in the list
        if let Some(focused) = &self.focused_pane {
            if let Some(idx) = self.panes.iter().position(|p| p.pane_info.id == focused.pane_info.id) {
                self.selected = idx;
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
    fn load(&mut self, _: BTreeMap<String, String>) {
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
                self.update_panes();
                should_render = true;
            }
            Event::PaneUpdate(pane_manifest) => {
                self.pane_manifest = Some(pane_manifest);
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
                    }
                }
            }
            Event::RunCommandResult(_exit_code, stdout, _stderr, context) => {
                if context.get("source").map(|s| s.as_str()) == Some("load") {
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
                }
            }
            Event::Key(key) => match key.bare_key {
                BareKey::Char('A') => {
                    // Add all terminal panes from all tabs that aren't already tracked
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
                    // Add the currently focused terminal pane if not already tracked.
                    // Since pane IDs are session-unique for terminal panes, we only
                    // need to check the ID (not tab position).
                    if let Some(pane) = &self.focused_pane {
                        if !self.panes.iter().any(|p| p.pane_info.id == pane.pane_info.id) {
                            self.panes.push(pane.clone());
                            self.sort_panes();
                            self.persistence
                                .save_to_disk(&self.session_name, &self.panes);
                        }
                    }
                    should_render = true;
                    hide_self();
                }
                BareKey::Char('d') => {
                    if self.selected < self.panes.len() {
                        self.panes.remove(self.selected);
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
                    if self.panes.len() > 0 {
                        self.select_down();
                        should_render = true;
                    }
                }
                BareKey::Up | BareKey::Char('k') => {
                    if self.panes.len() > 0 {
                        self.select_up();
                        should_render = true;
                    }
                }
                BareKey::Enter | BareKey::Char('l') => {
                    if let Some(pane) = self.panes.get(self.selected) {
                        hide_self();
                        // TODO: This has a bug on macOS with hidden panes
                        focus_terminal_pane(pane.pane_info.id, true);
                    }
                }
                _ => (),
            },
            _ => (),
        };

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        // Note: y=0 overlaps with the zellij pane frame/title bar and is not visible,
        // so we start rendering from y=1.
        let header = format!("==== {} panes ====", self.panes.len());
        let x = cols.saturating_sub(header.len()) / 2;
        print_text_with_coordinates(Text::new(&header), x, 0, None, None);
        let mut y = 1;

        for (idx, pane) in self.panes.iter().enumerate() {
            let text = if idx == self.selected {
                Text::new(&pane.to_string()).selected()
            } else {
                Text::new(&pane.to_string())
            };
            print_text_with_coordinates(text, 0, y, None, None);
            y += 1;
        }

        let hint_y = rows.saturating_sub(1);
        let hint_line = build_hint_line(cols);
        print_text_with_coordinates(hint_line, 0, hint_y, None, None);
    }
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
