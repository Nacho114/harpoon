use owo_colors::OwoColorize;
use zellij_tile::prelude::*;

struct State {
    tabs: Vec<TabInfo>,
    selected: Option<usize>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            tabs: Vec::default(),
            selected: Some(0),
        }
    }
}

impl State {
    fn reset_selection(&mut self) {
        let tabs: Vec<&TabInfo> = self.tabs.iter().collect();

        if tabs.is_empty() {
            self.selected = None
        } else if let Some(tab) = tabs.first() {
            self.selected = Some(tab.position)
        }
    }

    fn select_down(&mut self) {
        let tabs = self.tabs.iter();

        let mut can_select = false;
        let mut first = None;
        for TabInfo { position, .. } in tabs {
            if first.is_none() {
                first.replace(position);
            }

            if can_select {
                self.selected = Some(*position);
                return;
            } else if Some(*position) == self.selected {
                can_select = true;
            }
        }

        if let Some(position) = first {
            self.selected = Some(*position)
        }
    }

    fn select_up(&mut self) {
        let tabs = self.tabs.iter().rev();

        let mut can_select = false;
        let mut last = None;
        for TabInfo { position, .. } in tabs {
            if last.is_none() {
                last.replace(position);
            }

            if can_select {
                self.selected = Some(*position);
                return;
            } else if Some(*position) == self.selected {
                can_select = true;
            }
        }

        if let Some(position) = last {
            self.selected = Some(*position)
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        subscribe(&[EventType::TabUpdate, EventType::Key]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::TabUpdate(tab_info) => {
                self.tabs = tab_info;
                should_render = true;
            }

            Event::Key(Key::Esc | Key::Ctrl('c')) => {
                close_focus();
            }

            Event::Key(Key::Down | Key::Char('j')) => {
                self.select_down();

                should_render = true;
            }
            Event::Key(Key::Up | Key::Char('k')) => {
                self.select_up();

                should_render = true;
            }
            Event::Key(Key::Char('\n')) => {
                let tab = self
                    .tabs
                    .iter()
                    .find(|tab| Some(tab.position) == self.selected);

                if let Some(tab) = tab {
                    close_focus();
                    switch_tab_to(tab.position as u32 + 1);
                }
            }
            Event::Key(Key::Backspace) => {
                self.reset_selection();

                should_render = true;
            }
            Event::Key(Key::Char(c)) if c.is_ascii_alphabetic() || c.is_ascii_digit() => {
                self.reset_selection();

                should_render = true;
            }
            _ => (),
        };

        should_render
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        println!(
            "{}",
            self.tabs
                .iter()
                .map(|tab| {
                    let row = if tab.active {
                        format!("{}", tab.name).red().bold().to_string()
                    } else {
                        format!("{}", tab.name)
                    };

                    if Some(tab.position) == self.selected {
                        row.on_cyan().to_string()
                    } else {
                        row
                    }
                })
                .collect::<Vec<String>>()
                .join("\n")
        );
    }
}
