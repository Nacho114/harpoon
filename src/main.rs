use core::fmt;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Write, path::Path};

use owo_colors::OwoColorize;
use zellij_tile::prelude::*;

// TODO: This should probably come from a config file or something

// NOTE: These are hard coded and common to cached
const TAB_UPDATE_CACHE_PATH: &str = "/tmp/tab_update.json";
const PANE_MANIFEST_CACHE_PATH: &str = "/tmp/pane_manifest.json";

const HARPOON_CACHE_PATH: &str = "/tmp/harpoon.json";

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

// ----------------------------------- IO ------------------------------------------------

fn read_cached_panes() -> Vec<Pane> {
    if !Path::new(HARPOON_CACHE_PATH).exists() {
        return Vec::default();
    }

    let panes = std::fs::read_to_string(HARPOON_CACHE_PATH).unwrap();
    let panes: Vec<Pane> = serde_json::from_str(&panes).unwrap();

    return panes;
}

fn write_to_cache(panes: &Vec<Pane>) {
    let serialized = serde_json::to_string(panes).unwrap();
    let mut file = File::create(HARPOON_CACHE_PATH).unwrap();
    file.write_all(serialized.as_bytes()).unwrap();
}

// ----------------------------------- IO ------------------------------------------------

fn read_cached_tab_info() -> Option<Vec<TabInfo>> {
    if !Path::new(TAB_UPDATE_CACHE_PATH).exists() {
        return None;
    }

    let panes = std::fs::read_to_string(TAB_UPDATE_CACHE_PATH).unwrap();
    let panes: Vec<TabInfo> = serde_json::from_str(&panes).unwrap();

    return Some(panes);
}

fn read_cached_pane_manifest() -> Option<PaneManifest> {
    if !Path::new(PANE_MANIFEST_CACHE_PATH).exists() {
        return None;
    }

    let pane_manifest = std::fs::read_to_string(PANE_MANIFEST_CACHE_PATH).unwrap();
    let pane_manifest: PaneManifest = serde_json::from_str(&pane_manifest).unwrap();

    return Some(pane_manifest);
}

// ----------------------------------- Find focused items -----------------------------------------

// TODO: These could be pushed upstream to Zellij

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

// ----------------------------------- Update ------------------------------------------------

fn update_panes(
    panes: Vec<Pane>,
    pane_manifest: PaneManifest,
    tab_infos: Vec<TabInfo>,
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

fn init_state() -> Option<State> {
    // Read cached info
    let tab_infos = read_cached_tab_info()?;
    let pane_manifest = read_cached_pane_manifest()?;

    // Get focused pane
    let tab_info = get_focused_tab(&tab_infos)?;
    let pane_info = get_focused_pane(tab_info.position, &pane_manifest)?;
    let focused_pane = Some(Pane {
        pane_info,
        tab_info,
    });

    // Get cached panes
    let panes = read_cached_panes();
    let panes = update_panes(panes, pane_manifest, tab_infos);

    // Put selected on middle of the options
    let selected = panes.len() / 2;

    Some(State {
        selected,
        panes,
        focused_pane,
    })
}

struct State {
    selected: usize,
    panes: Vec<Pane>,
    focused_pane: Option<Pane>,
}

impl Default for State {
    fn default() -> Self {
        if let Some(state) = init_state() {
            state
        } else {
            Self {
                selected: 0,
                panes: Vec::default(),
                focused_pane: None,
            }
        }
    }
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
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        subscribe(&[EventType::Key]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Key(Key::Char('a')) => {
                let panes_ids: Vec<u32> = self.panes.iter().map(|p| p.pane_info.id).collect();
                if let Some(pane) = &self.focused_pane {
                    if !panes_ids.contains(&pane.pane_info.id) {
                        self.panes.push(pane.clone());
                        self.sort_panes();
                        write_to_cache(&self.panes);
                    }
                }
                close_focus();
            }

            Event::Key(Key::Char('d')) => {
                if self.selected < self.panes.len() {
                    self.panes.remove(self.selected);
                    write_to_cache(&self.panes);
                }

                if self.panes.len() > 0 {
                    self.select_up();
                }
                should_render = true;
            }

            Event::Key(Key::Esc | Key::Ctrl('c')) => {
                close_focus();
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
                    close_focus();
                    // TODO: This has a bug on macOS with hidden panes
                    focus_terminal_pane(pane.pane_info.id as i32, true);
                }
            }
            Event::Key(Key::Backspace) => {
                should_render = true;
            }

            Event::Key(Key::Char(c)) if c.is_ascii_alphabetic() || c.is_ascii_digit() => {
                should_render = true;
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
