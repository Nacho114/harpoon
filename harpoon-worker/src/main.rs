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

/// Map of (tab_name, pane_title) -> last_accessed timestamp.
/// We use tab_name+pane_title as key since pane IDs are not stable across sessions.
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
    tab_info: Option<Vec<TabInfo>>,
    pane_manifest: Option<PaneManifest>,
    session_name: Option<String>,
    last_focused_id: Option<u32>,
    timestamps: TimestampMap,
    debug: bool,
}

impl State {
    fn data_dir() -> String {
        "${XDG_DATA_HOME:-$HOME/.local/share}/zellij-harpoon".to_string()
    }

    fn timestamps_file(session_name: &str) -> String {
        format!("{}/{}-timestamps.json", Self::data_dir(), session_name)
    }

    fn log_file(session_name: &str) -> String {
        format!("{}/{}-debug.log", Self::data_dir(), session_name)
    }

    fn debug_log(&self, msg: &str) {
        if !self.debug {
            return;
        }
        let Some(session) = &self.session_name else { return };
        let log_path = Self::log_file(session);
        let ts = now_secs();
        let line = format!("[{}] {}", ts, msg);
        let cmd = format!(
            "mkdir -p {} && echo '{}' >> {}",
            Self::data_dir(),
            line,
            log_path,
        );
        let mut ctx = BTreeMap::new();
        ctx.insert("source".to_string(), "log".to_string());
        run_command(&["sh", "-c", &cmd], ctx);
    }

    fn save_timestamps(&self) {
        let Some(session) = &self.session_name else { return };
        let json = serde_json::to_string(&self.timestamps).unwrap_or_default();
        let file_path = Self::timestamps_file(session);
        let cmd = format!(
            "mkdir -p {} && printf '%s' \"$1\" > {}",
            Self::data_dir(),
            file_path,
        );
        let mut ctx = BTreeMap::new();
        ctx.insert("source".to_string(), "save".to_string());
        run_command(&["sh", "-c", &cmd, "_", &json], ctx);
    }

    fn update_focus(&mut self) {
        let tab_info = match &self.tab_info {
            Some(t) => t,
            None => return,
        };
        let pane_manifest = match &self.pane_manifest {
            Some(p) => p,
            None => return,
        };

        let focused_tab = match tab_info.iter().find(|t| t.active) {
            Some(t) => t,
            None => return,
        };

        let panes = match pane_manifest.panes.get(&focused_tab.position) {
            Some(p) => p,
            None => return,
        };

        // Find the focused terminal pane
        let focused_pane = panes.iter().find(|p| p.is_focused && !p.is_plugin)
            .or_else(|| panes.iter().find(|p| !p.is_plugin));

        let Some(focused_pane) = focused_pane else { return };

        // Only update if focus actually changed
        if self.last_focused_id == Some(focused_pane.id) {
            return;
        }
        self.last_focused_id = Some(focused_pane.id);

        let tab_name = &focused_tab.name;
        let pane_title = &focused_pane.title;
        let pane_id = focused_pane.id;
        let ts = now_secs();

        self.debug_log(&format!(
            "focus changed: pane_id={} tab=\"{}\" title=\"{}\" ts={}",
            pane_id, tab_name, pane_title, ts
        ));

        // Update or insert, matching by pane_id first (skip legacy entries with pane_id=0)
        if let Some(entry) = self.timestamps.entries.iter_mut().find(|e| e.pane_id != 0 && e.pane_id == pane_id) {
            entry.last_accessed = ts;
            entry.tab_name = tab_name.clone();
            entry.pane_title = pane_title.clone();
        } else {
            // Remove stale legacy entry with same tab_name+pane_title
            self.timestamps.entries.retain(|e| {
                !(e.pane_id == 0 && e.tab_name == *tab_name && e.pane_title == *pane_title)
            });
            self.timestamps.entries.push(TimestampEntry {
                tab_name: tab_name.clone(),
                pane_title: pane_title.clone(),
                pane_id,
                last_accessed: ts,
            });
        }

        self.save_timestamps();
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.debug = configuration
            .get("debug")
            .map(|v| v == "true")
            .unwrap_or(false);
        request_permission(&[
            PermissionType::RunCommands,
            PermissionType::ReadApplicationState,
        ]);
        subscribe(&[
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::SessionUpdate,
            EventType::RunCommandResult,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tab_info) => {
                self.tab_info = Some(tab_info);
                self.update_focus();
            }
            Event::PaneUpdate(pane_manifest) => {
                self.pane_manifest = Some(pane_manifest);
                self.update_focus();
            }
            Event::SessionUpdate(session_infos, _) => {
                if self.session_name.is_none() {
                    if let Some(current) = session_infos.iter().find(|s| s.is_current_session) {
                        self.session_name = Some(current.name.clone());
                        // Load existing timestamps
                        let file_path = Self::timestamps_file(&current.name);
                        let cmd = format!("cat {} 2>/dev/null || echo '{{}}'", file_path);
                        let mut ctx = BTreeMap::new();
                        ctx.insert("source".to_string(), "load".to_string());
                        run_command(&["sh", "-c", &cmd], ctx);
                    }
                }
            }
            Event::RunCommandResult(_exit_code, stdout, _stderr, context) => {
                if context.get("source").map(|s| s.as_str()) == Some("load") {
                    let content = String::from_utf8_lossy(&stdout);
                    if let Ok(map) = serde_json::from_str::<TimestampMap>(&content) {
                        self.timestamps = map;
                    }
                }
            }
            _ => {}
        }
        false // never needs rendering
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Worker is invisible, no rendering needed
    }
}
