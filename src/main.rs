use core::fmt;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Write, path::Path};

use owo_colors::OwoColorize;
use zellij_tile::prelude::*;

// ---------------- IO -----------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: u32,
    pub title: String,
    pub tab: Tab,
}

impl fmt::Display for Pane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} | {} | id: {}", self.tab.name, self.title, self.id)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tab {
    pub name: String,
    pub position: usize,
}

impl Tab {
    fn write_cache(&self) {
        let serialized = serde_json::to_string(self).unwrap();
        let mut file = File::create(INFO_PATH).unwrap();
        file.write_all(serialized.as_bytes()).unwrap();
    }
}

//TODO: Change path
const CACHE_PATH: &str = "/host/harpoon_panes.json";
const INFO_PATH: &str = "/host/harpoon_info.json";

//TODO: name
pub trait Panes {
    fn write_cache(&self) -> ();
    fn get_ids(&self) -> Vec<u32>;
}

impl Panes for Vec<Pane> {
    fn write_cache(&self) {
        let serialized = serde_json::to_string(self).unwrap();
        let mut file = File::create(CACHE_PATH).unwrap();
        file.write_all(serialized.as_bytes()).unwrap();
    }

    fn get_ids(&self) -> Vec<u32> {
        let ids: Vec<u32> = self.iter().map(|p| p.id).collect();
        ids
    }
}

fn read_cached_panes() -> Vec<Pane> {
    if !Path::new(CACHE_PATH).exists() {
        return Vec::default();
    }

    let panes = std::fs::read_to_string(CACHE_PATH).unwrap();
    let panes: Vec<Pane> = serde_json::from_str(&panes).unwrap();

    return panes;
}

fn read_cached_tab() -> Option<Tab> {
    if !Path::new(INFO_PATH).exists() {
        return None;
    }

    let tab = std::fs::read_to_string(INFO_PATH).unwrap();
    let tab: Tab = serde_json::from_str(&tab).unwrap();

    return Some(tab);
}

// ---------------- Plugin -----------------------

fn get_focused_tab(tab_info: Vec<TabInfo>) -> Option<TabInfo> {
    for tab in tab_info {
        if tab.active {
            return Some(tab);
        }
    }
    return None;
}

fn is_user_pane(pane: &PaneInfo) -> bool {
    // TODO: Which things should I check for
    pane.is_focused & !pane.is_plugin
}

fn get_focused_pane(tab_position: usize, pane_manifest: PaneManifest) -> Option<PaneInfo> {
    let panes = pane_manifest.panes.get(&tab_position);
    if let Some(panes) = panes {
        for pane in panes {
            if is_user_pane(pane) {
                return Some(pane.clone());
            }
        }
    }
    None
}

struct State {
    selected: usize,
    panes: Vec<Pane>,
    focused_tab: Option<Tab>,
    focused_pane: Option<Pane>,
    counter: usize,
}

impl Default for State {
    fn default() -> Self {
        let panes = read_cached_panes();
        let selected = panes.len() / 2;
        Self {
            selected,
            panes,
            focused_tab: read_cached_tab(),
            focused_pane: None,
            counter: 0,
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

    fn set_focused_pane(&mut self, pane_manifest: PaneManifest) -> Option<Pane> {
        let focused_tab = self.focused_tab.clone()?;
        let focused_pane = get_focused_pane(focused_tab.position.clone(), pane_manifest)?;
        let pane = Pane {
            id: focused_pane.id,
            title: focused_pane.title,
            tab: focused_tab,
        };
        self.focused_pane = Some(pane.clone());
        Some(pane)
    }

    fn update_panes(&mut self, pane_manifest: PaneManifest) {
        let mut new_panes: Vec<Pane> = Vec::default();
        for pane in self.panes.clone() {
            if let Some(other_panes) = pane_manifest.panes.get(&pane.tab.position) {
                if let Some(matching_pane) = other_panes
                    .iter()
                    .find(|p| (p.id == pane.id) & is_user_pane(p))
                {
                    let new_pane = Pane {
                        title: matching_pane.title.clone(),
                        ..pane
                    };
                    new_panes.push(new_pane);
                }
            }
        }
        self.panes = new_panes;
        self.panes.write_cache();
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        subscribe(&[EventType::TabUpdate, EventType::Key, EventType::PaneUpdate]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::TabUpdate(tab_info) => {
                let tab = get_focused_tab(tab_info);
                if let Some(tab) = tab {
                    let tab = Tab {
                        name: tab.name,
                        position: tab.position,
                    };
                    self.focused_tab = Some(tab.clone());
                    tab.write_cache();
                }
            }

            Event::PaneUpdate(pane_manifest) => {
                self.set_focused_pane(pane_manifest.clone());
                // self.update_panes(pane_manifest);
            }

            Event::Key(Key::Char('a')) => {
                // TODO: There is a bug when trying to add just after opening a new tab
                if let Some(pane) = &self.focused_pane {
                    if !self.panes.get_ids().contains(&pane.id) {
                        self.panes.push(pane.clone());
                        self.panes.write_cache();
                        should_render = true;
                        self.counter = self.counter + 1;
                    }
                }
                hide_self();
            }

            Event::Key(Key::Char('d')) => {
                if self.selected < self.panes.len() {
                    self.panes.remove(self.selected);
                    self.panes.write_cache();
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
                    focus_terminal_pane(pane.id as i32, true);
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
        println!("So far: {}", self.counter);
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
