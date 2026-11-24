// SPDX-License-Identifier: GPL-3.0-or-later
//! A tree of folders: the rows of it, those to draw with a filter or as they are open, and drawing them.

use crate::library::norm_key;
use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A folder in the tree.
pub struct Folder {
    pub path: PathBuf,
    pub key: String,
    pub name: String,
    pub depth: usize,
    /// Its row.
    pub parent: Option<usize>,
    pub kids: bool,
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The folders under the roots, depth first, without the hidden ones.
pub fn rows(roots: &[PathBuf], folders: &[PathBuf], hidden: &HashSet<String>) -> Vec<Folder> {
    fn add(out: &mut Vec<Folder>, children: &HashMap<String, Vec<PathBuf>>, path: &Path, depth: usize, parent: Option<usize>) {
        let key = norm_key(path);
        let kids = children.get(&key);
        let at = out.len();
        out.push(Folder { path: path.to_path_buf(), key, name: file_name(path), depth, parent, kids: kids.is_some() });
        for kid in kids.into_iter().flatten() {
            add(out, children, kid, depth + 1, Some(at));
        }
    }
    let roots_keys: HashSet<String> = roots.iter().map(|r| norm_key(r)).collect();
    let mut children: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for folder in folders {
        let key = norm_key(folder);
        if !hidden.contains(&key)
            && !roots_keys.contains(&key)
            && let Some(parent) = folder.parent()
        {
            children.entry(norm_key(parent)).or_default().push(folder.clone());
        }
    }
    let mut out = Vec::new();
    for root in roots {
        add(&mut out, &children, root, 0, None);
    }
    out
}

/// Puts a new folder in the tree, the first one of its parent.
pub fn insert(folders: &mut Vec<Folder>, parent: &Path, path: &Path) {
    let (key, parent_key) = (norm_key(path), norm_key(parent));
    let Some(at) = folders.iter().position(|f| f.key == parent_key) else { return };
    if folders.iter().any(|f| f.key == key) {
        return;
    }
    let depth = folders[at].depth + 1;
    folders[at].kids = true;
    for folder in folders.iter_mut() {
        folder.parent = folder.parent.map(|p| if p > at { p + 1 } else { p });
    }
    folders.insert(at + 1, Folder { path: path.to_path_buf(), key, name: file_name(path), depth, parent: Some(at), kids: false });
}

/// The folders to draw: those whose parents are open, or, with a filter, those that match and their parents.
pub fn visible(folders: &[Folder], expanded: &HashSet<String>, filter: &str) -> Vec<usize> {
    let words: Vec<String> = filter.to_lowercase().split_whitespace().map(str::to_string).collect();
    let mut show = vec![false; folders.len()];
    for i in 0..folders.len() {
        let folder = &folders[i];
        if words.is_empty() {
            show[i] = folder.parent.is_none_or(|p| show[p] && expanded.contains(&folders[p].key));
        } else if words.iter().all(|w| folder.name.to_lowercase().contains(w.as_str())) {
            let mut at = Some(i);
            while let Some(n) = at {
                show[n] = true;
                at = folders[n].parent;
            }
        }
    }
    (0..folders.len()).filter(|&i| show[i]).collect()
}

/// Draws the tree; `true` when a folder was chosen.
pub fn show(ui: &mut egui::Ui, id: &str, folders: &[Folder], expanded: &mut HashSet<String>, filter: &str, chosen: &mut Option<PathBuf>) -> bool {
    let list = visible(folders, expanded, filter);
    let height = ui.spacing().interact_size.y;
    let current = chosen.as_deref().map(norm_key);
    let mut clicked = false;
    egui::ScrollArea::both().id_salt(id).auto_shrink(false).show_rows(ui, height, list.len(), |ui, range| {
        for &i in &list[range] {
            let folder = &folders[i];
            ui.horizontal(|ui| {
                ui.add_space(folder.depth as f32 * 14.0);
                let open = expanded.contains(&folder.key);
                if folder.kids {
                    if ui.small_button(if open { "−" } else { "+" }).clicked() {
                        if open {
                            expanded.remove(&folder.key);
                        } else {
                            expanded.insert(folder.key.clone());
                        }
                    }
                } else {
                    ui.add_space(18.0);
                }
                if ui.selectable_label(current.as_ref() == Some(&folder.key), &folder.name).clicked() {
                    *chosen = Some(folder.path.clone());
                    clicked = true;
                }
            });
        }
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<Folder> {
        let folders: Vec<PathBuf> = ["S:\\Lib", "S:\\Lib\\Chairs", "S:\\Lib\\Chairs\\Oak", "S:\\Lib\\z-maps"].iter().map(PathBuf::from).collect();
        rows(&["S:\\Lib".into()], &folders, &HashSet::from([norm_key(Path::new("S:\\Lib\\z-maps"))]))
    }

    #[test]
    fn the_hidden_folders_are_left_out() {
        let rows: Vec<(String, usize, bool)> = tree().into_iter().map(|f| (f.name, f.depth, f.kids)).collect();
        assert_eq!(rows, [("Lib".to_string(), 0, true), ("Chairs".to_string(), 1, true), ("Oak".to_string(), 2, false)]);
    }

    #[test]
    fn a_filter_shows_what_matches_and_where_it_is() {
        let rows = tree();
        let open = HashSet::from([rows[0].key.clone()]);
        assert_eq!(visible(&rows, &open, ""), [0, 1]);
        assert_eq!(visible(&rows, &open, "oak"), [0, 1, 2]);
        assert_eq!(visible(&rows, &HashSet::new(), ""), [0]);
        assert!(visible(&rows, &open, "nothing").is_empty());
    }

    #[test]
    fn a_new_folder_keeps_the_rows_of_the_others() {
        let mut rows = tree();
        insert(&mut rows, Path::new("S:\\Lib"), Path::new("S:\\Lib\\New"));
        let names: Vec<(&str, Option<usize>)> = rows.iter().map(|f| (f.name.as_str(), f.parent)).collect();
        assert_eq!(names, [("Lib", None), ("New", Some(0)), ("Chairs", Some(0)), ("Oak", Some(2))]);
    }
}
