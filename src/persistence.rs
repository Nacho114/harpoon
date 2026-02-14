use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zellij_tile::prelude::*;

use crate::Pane;

#[derive(Clone, Serialize, Deserialize)]
struct PaneBookmark {
    tab_name: String,
    pane_title: String,
}

#[derive(Default)]
pub struct Persistence {
    pending_bookmarks: Vec<PaneBookmark>,
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
                Ok(())
            }
            Err(e) => Err(PersistenceError::LoadFromDiskFailed(e)),
        }
    }

    pub fn save_to_disk(&self, session_name: &Option<String>, panes: &[Pane]) {
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
