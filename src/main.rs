use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use zellij_tile::prelude::*;

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

fn get_focused_tab(tab_infos: &Vec<TabInfo>) -> Option<TabInfo> {
    for tab in tab_infos {
        if tab.active {
            return Some(tab.clone());
        }
    }
    return None;
}

fn get_focused_pane(tab_position: usize, pane_manifest: &PaneManifest) -> Option<PaneInfo> {
    let panes = pane_manifest.panes.get(&tab_position)?;
    // First, try to find a focused non-plugin pane
    for pane in panes {
        if pane.is_focused && !pane.is_plugin {
            return Some(pane.clone());
        }
    }
    // Fallback: if no focused non-plugin pane (e.g., plugin has focus),
    // return the first non-plugin pane
    for pane in panes {
        if !pane.is_plugin {
            return Some(pane.clone());
        }
    }
    None
}

//--------->

// ----------------------------------- Update ------------------------------------------------

fn get_valid_panes(
    panes: &Vec<Pane>,
    pane_manifest: &PaneManifest,
    tab_infos: &Vec<TabInfo>,
) -> Vec<Pane> {
    let mut new_panes: Vec<Pane> = Vec::default();
    for pane in panes.clone() {
        // Iterate over all panes, and find corresponding tab and pane based on id
        // update it in case the info has changed, and if they are not there do not add them.
        if let Some(tab_info) = tab_infos.iter().find(|t| t.position == pane.tab_info.position) {
            if let Some(other_panes) = pane_manifest.panes.get(&pane.tab_info.position) {
                if let Some(pane_info) = other_panes
                    .iter()
                    .find(|p| !p.is_plugin & (p.id == pane.pane_info.id))
                {
                    let pane_info = pane_info.clone();
                    let tab_info = tab_info.clone();
                    let new_pane = Pane {
                        pane_info,
                        tab_info,
                    };
                    new_panes.push(new_pane);
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
}

impl State {
    fn select_down(&mut self) {
        self.selected = (self.selected + 1) % self.panes.len();
    }

    fn select_up(&mut self) {
        if self.selected == 0 {
            self.selected = self.panes.len() - 1;
            return;
        }
        self.selected = self.selected - 1;
    }

    fn sort_panes(&mut self) {
        self.panes.sort_by(|x, y| {
            (x.tab_info.position)
                .partial_cmp(&y.tab_info.position)
                .unwrap()
        });
    }

    /// Update panes updates the pane states based on the latest pane_manifest and tab_info
    fn update_panes(&mut self) -> Option<()> {
        // Update panes to filter our invalid panes (e.g. tab/pane was closed).
        let pane_manifest = self.pane_manifest.clone()?;
        let tab_info = self.tab_info.clone()?;
        let panes = get_valid_panes(&self.panes.clone(), &pane_manifest, &tab_info);
        self.panes = panes;

        // Update currently focused pane
        let tab_info = get_focused_tab(&tab_info)?;
        let pane_info = get_focused_pane(tab_info.position, &pane_manifest)?;
        self.focused_pane = Some(Pane {
            pane_info,
            tab_info,
        });

        // Set default location of selected idx to currently focused pane
        if let Some(focused_pane) = &self.focused_pane {
            for (idx,pane) in self.panes.iter().enumerate() {
                if pane.pane_info.id == focused_pane.pane_info.id {
                    self.selected = idx;
                }
            }
        }else{
            self.selected = 0;
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
        subscribe(&[EventType::Key, EventType::TabUpdate, EventType::PaneUpdate]);

        let plugin_ids = get_plugin_ids();
        rename_plugin_pane(plugin_ids.plugin_id, "harpoon");
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
            Event::Key(key) => match key.bare_key {
                BareKey::Char('A') => {
                    let current_pane_ids: Vec<u32> =
                        self.panes.iter().map(|p| p.pane_info.id).collect();
                    if let Some(pane_manifest) = &self.pane_manifest {
                        if let Some(tab_info) = &self.tab_info {
                            for (tab_position, panes) in &pane_manifest.panes {
                                if let Some(tab) =
                                    tab_info.iter().find(|t| t.position == *tab_position)
                                {
                                    for pane in panes {
                                        if !pane.is_plugin && !current_pane_ids.contains(&pane.id) {
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
                    should_render = true;
                    hide_self();
                }
                BareKey::Char('a') => {
                    let panes_ids: Vec<u32> = self.panes.iter().map(|p| p.pane_info.id).collect();
                    if let Some(pane) = &self.focused_pane {
                        if !panes_ids.contains(&pane.pane_info.id) {
                            self.panes.push(pane.clone());
                            self.sort_panes();
                        }
                    }
                    should_render = true;
                    hide_self();
                }
                BareKey::Char('d') => {
                    if self.selected < self.panes.len() {
                        self.panes.remove(self.selected);
                    }
                    if self.panes.len() > 0 {
                        self.select_up();
                    }
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
                    let pane = self.panes.get(self.selected);

                    if let Some(pane) = pane {
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
        for (idx, pane) in self.panes.iter().enumerate() {
            let text = if idx == self.selected {
                Text::new(pane.to_string()).selected()
            } else {
                Text::new(pane.to_string())
            };
            print_text_with_coordinates(text, 0, idx, None, None);
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

fn build_hint_string(parts: &[(&str, &str)], separator: &str) -> (String, Vec<std::ops::Range<usize>>) {
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
