use core::fmt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use owo_colors::OwoColorize;
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
    let panes = pane_manifest.panes.get(&tab_position);
    if let Some(panes) = panes {
        for pane in panes {
            if pane.is_focused & !pane.is_plugin {
                return Some(pane.clone());
            }
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
        if let Some(tab_info) = tab_infos.get(pane.tab_info.position) {
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

        // Set default location of selected idx to the center
        self.selected = self.panes.len() / 2;
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
            Event::Key(Key::Char('a')) => {
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
            Event::Key(Key::Char('d')) => {
                if self.selected < self.panes.len() {
                    self.panes.remove(self.selected);
                }
                if self.panes.len() > 0 {
                    self.select_up();
                }
                should_render = true;
            }

            Event::Key(Key::Esc | Key::Ctrl('c')) => {
                hide_self();
            }

            Event::Key(Key::Down | Key::Char('j')) => {
                if self.panes.len() > 0 {
                    self.select_down();
                    should_render = true;
                }
            }
            Event::Key(Key::Up | Key::Char('k')) => {
                if self.panes.len() > 0 {
                    self.select_up();
                    should_render = true;
                }
            }
            Event::Key(Key::Char('\n') | Key::Char('l')) => {
                let pane = self.panes.get(self.selected);

                if let Some(pane) = pane {
                    hide_self();
                    // TODO: This has a bug on macOS with hidden panes
                    focus_terminal_pane(pane.pane_info.id, true);
                }
            }
            _ => (),
        };

        should_render
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!(
            "{}",
            self.panes
                .iter()
                .enumerate()
                .map(|(idx, pane)| {
                    if idx == self.selected {
                        pane.to_string().red().bold().to_string()
                    } else {
                        pane.to_string()
                    }
                })
                .collect::<Vec<String>>()
                .join("\n")
        );
    }
}
