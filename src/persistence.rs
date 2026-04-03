use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zellij_tile::prelude::*;

use crate::Pane;

#[derive(Clone, Serialize, Deserialize)]
struct PaneBookmark {
    tab_name: String,
    pane_title: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct HarpoonConfig {
    #[serde(default)]
    pub cross_session: bool,
}

/// A pane loaded from another session's bookmark file.
#[derive(Clone)]
pub struct RemotePane {
    pub session_name: String,
    pub tab_name: String,
    pub pane_title: String,
}

#[derive(Default)]
pub struct Persistence {
    pending_bookmarks: Vec<PaneBookmark>,
    last_saved_state: Vec<(String, String)>,
    pub config: HarpoonConfig,
    pub remote_panes: Vec<RemotePane>,
}

#[derive(Debug)]
pub enum PersistenceError {
    LoadFromDiskFailed(serde_json::Error),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::LoadFromDiskFailed(e) => {
                write!(f, "Failed to load session from disk: {e:?}")
            }
        }
    }
}

impl Persistence {
    pub fn match_pending_bookmarks(
        &mut self,
        panes: &[Pane],
        pane_manifest: &PaneManifest,
        tab_infos: &[TabInfo],
    ) -> Vec<Pane> {
        if self.pending_bookmarks.is_empty() {
            return Vec::new();
        }

        let mut current_pane_ids: Vec<u32> = panes.iter().map(|p| p.pane_info.id).collect();
        let mut new_panes: Vec<Pane> = Vec::new();

        self.pending_bookmarks.retain(|bookmark| {
            match find_pane_for_bookmark(bookmark, pane_manifest, tab_infos, &current_pane_ids) {
                Some(pane) => {
                    current_pane_ids.push(pane.pane_info.id);
                    new_panes.push(pane);
                    false
                }
                None => true,
            }
        });

        new_panes
    }

    pub fn has_changed(&self, panes: &[Pane]) -> bool {
        let current: Vec<(String, String)> = panes
            .iter()
            .map(|p| (p.tab_info.name.clone(), p.pane_info.title.clone()))
            .collect();
        current != self.last_saved_state
    }

    fn data_dir_path(&self) -> String {
        "${XDG_DATA_HOME:-$HOME/.local/share}/zellij-harpoon".to_string()
    }

    fn session_file_path(&self, session_name: &Option<String>) -> Option<String> {
        let session = session_name.as_ref()?;
        Some(format!("{}/{}.json", self.data_dir_path(), session))
    }

    pub fn load_from_disk(&self, session_name: &Option<String>) {
        let Some(file_path) = self.session_file_path(session_name) else {
            return;
        };
        let cmd = format!("cat {file_path} 2>/dev/null || echo '[]'");
        let mut context = BTreeMap::new();
        context.insert("source".to_string(), "load".to_string());
        run_command(&["sh", "-c", &cmd], context);
    }

    pub fn on_load_command(&mut self, content: &str) -> Result<(), PersistenceError> {
        match serde_json::from_str::<Vec<PaneBookmark>>(content) {
            Ok(bookmarks) => {
                self.pending_bookmarks = bookmarks;
                self.last_saved_state = self
                    .pending_bookmarks
                    .iter()
                    .map(|b| (b.tab_name.clone(), b.pane_title.clone()))
                    .collect();
                Ok(())
            }
            Err(e) => Err(PersistenceError::LoadFromDiskFailed(e)),
        }
    }

    pub fn load_config(&self) {
        let cmd = format!(
            "cat {}/config.json 2>/dev/null || echo '{{}}'",
            self.data_dir_path()
        );
        let mut context = BTreeMap::new();
        context.insert("source".to_string(), "load_config".to_string());
        run_command(&["sh", "-c", &cmd], context);
    }

    pub fn on_load_config_command(&mut self, content: &str) {
        self.config = serde_json::from_str::<HarpoonConfig>(content).unwrap_or_default();
    }

    pub fn save_config(&self) {
        let json =
            serde_json::to_string(&self.config).unwrap_or_else(|_| "{}".to_string());
        let cmd = format!(
            "mkdir -p {} && printf '%s' \"$1\" > {}/config.json",
            self.data_dir_path(),
            self.data_dir_path(),
        );
        let mut context = BTreeMap::new();
        context.insert("source".to_string(), "save_config".to_string());
        run_command(&["sh", "-c", &cmd, "_", &json], context);
    }

    pub fn load_remote_panes(&self, current_session: &Option<String>) {
        let current = current_session.as_deref().unwrap_or("");
        let cmd = format!(
            "for f in {dir}/*.json; do \
                name=\"$(basename \"$f\" .json)\"; \
                [ \"$name\" = \"config\" ] && continue; \
                [ \"$name\" = \"{current}\" ] && continue; \
                echo \"SESSION:$name\"; \
                cat \"$f\"; \
                echo; \
            done 2>/dev/null || true",
            dir = self.data_dir_path(),
            current = current,
        );
        let mut context = BTreeMap::new();
        context.insert("source".to_string(), "load_remote".to_string());
        run_command(&["sh", "-c", &cmd], context);
    }

    pub fn on_load_remote_command(&mut self, content: &str) {
        self.remote_panes.clear();
        let mut current_session = String::new();
        for line in content.lines() {
            if let Some(name) = line.strip_prefix("SESSION:") {
                current_session = name.to_string();
            } else if !line.is_empty() && !current_session.is_empty() {
                if let Ok(bookmarks) = serde_json::from_str::<Vec<PaneBookmark>>(line) {
                    for b in bookmarks {
                        self.remote_panes.push(RemotePane {
                            session_name: current_session.clone(),
                            tab_name: b.tab_name,
                            pane_title: b.pane_title,
                        });
                    }
                }
            }
        }
    }

    pub fn save_to_disk(&mut self, session_name: &Option<String>, panes: &[Pane]) {
        let Some(file_path) = self.session_file_path(session_name) else {
            return;
        };
        let bookmarks: Vec<PaneBookmark> = panes
            .iter()
            .map(|p| PaneBookmark {
                tab_name: p.tab_info.name.clone(),
                pane_title: p.pane_info.title.clone(),
            })
            .collect();

        let json = serde_json::to_string(&bookmarks).unwrap_or_else(|_| "[]".to_string());
        let cmd = format!(
            "mkdir -p {} && printf '%s' \"$1\" > {}",
            self.data_dir_path(),
            file_path,
        );
        let mut context = BTreeMap::new();
        context.insert("source".to_string(), "save".to_string());
        run_command(&["sh", "-c", &cmd, "_", &json], context);

        self.last_saved_state = bookmarks
            .iter()
            .map(|b| (b.tab_name.clone(), b.pane_title.clone()))
            .collect();
    }
}

fn find_pane_for_bookmark(
    bookmark: &PaneBookmark,
    pane_manifest: &PaneManifest,
    tab_infos: &[TabInfo],
    current_pane_ids: &[u32],
) -> Option<Pane> {
    for (tab_position, panes) in &pane_manifest.panes {
        let Some(tab) = tab_infos.iter().find(|t| t.position == *tab_position) else {
            continue;
        };
        if tab.name != bookmark.tab_name {
            continue;
        }

        let matched_pane = panes
            .iter()
            .find(|p| {
                !p.is_plugin && p.title == bookmark.pane_title && !current_pane_ids.contains(&p.id)
            })
            .map(|pane| Pane {
                pane_info: pane.clone(),
                tab_info: tab.clone(),
            });

        if matched_pane.is_some() {
            return matched_pane;
        }
    }
    None
}
