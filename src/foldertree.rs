// SPDX-License-Identifier: GPL-3.0-or-later
//! A tree of folders: the rows of it, those to draw with a filter or as they are open, and drawing them.

use crate::favorites::TabView;
use crate::foldericons;
use crate::library::norm_key;
use crate::open3d_settings::Language;
use crate::settings::Settings;
use crate::texts::Text;
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
    /// How many items it and the folders in it have, when they are counted.
    pub count: Option<usize>,
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
        out.push(Folder { path: path.to_path_buf(), key, name: file_name(path), depth, parent, kids: kids.is_some(), count: None });
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
    folders.insert(at + 1, Folder { path: path.to_path_buf(), key, name: file_name(path), depth, parent: Some(at), kids: false, count: None });
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

/// How many items are in each folder and the folders in it, from the folder of each item.
pub fn totals<'a>(roots: &[PathBuf], items: impl Iterator<Item = &'a Path>) -> HashMap<String, usize> {
    let roots: HashSet<String> = roots.iter().map(|r| norm_key(r)).collect();
    let mut direct: HashMap<PathBuf, usize> = HashMap::new();
    for folder in items {
        *direct.entry(folder.to_path_buf()).or_default() += 1;
    }
    let mut out = HashMap::new();
    for (folder, count) in direct {
        for at in folder.ancestors() {
            let key = norm_key(at);
            let root = roots.contains(&key);
            *out.entry(key).or_default() += count;
            if root {
                break;
            }
        }
    }
    out
}

/// What was done in a tree.
enum Done {
    Chosen(PathBuf),
    Mark(PathBuf, &'static str, Option<&'static str>),
    Hide(PathBuf),
    HideName(String),
    Reveal(PathBuf),
    CloseAll,
}

/// A folder's icon and menu come from the settings; without them there are none.
type Marks<'a> = Option<(&'a Settings, Language)>;

/// Draws the tree; `true` when a folder was chosen.
pub fn show(ui: &mut egui::Ui, id: &str, folders: &[Folder], expanded: &mut HashSet<String>, filter: &str, chosen: &mut Option<PathBuf>) -> bool {
    let done = draw(ui, id, folders, expanded, filter, chosen.as_deref(), None);
    let Some(Done::Chosen(path)) = done else { return false };
    *chosen = Some(path);
    true
}

fn draw(
    ui: &mut egui::Ui,
    id: &str,
    folders: &[Folder],
    expanded: &mut HashSet<String>,
    filter: &str,
    chosen: Option<&Path>,
    marks: Marks<'_>,
) -> Option<Done> {
    let list = visible(folders, expanded, filter);
    let height = ui.spacing().interact_size.y;
    let current = chosen.map(norm_key);
    let mut done = None;
    egui::ScrollArea::both().id_salt(id).auto_shrink(false).show_rows(ui, height, list.len(), |ui, range| {
        for &i in &list[range] {
            let folder = &folders[i];
            ui.horizontal(|ui| {
                ui.add_space(folder.depth as f32 * 14.0);
                let open = expanded.contains(&folder.key);
                if folder.kids {
                    if ui.small_button(if open { "−" } else { "+" }).clicked() {
                        if ui.input(|i| i.modifiers.shift) {
                            done = Some(Done::CloseAll);
                        } else if open {
                            expanded.remove(&folder.key);
                        } else {
                            expanded.insert(folder.key.clone());
                        }
                    }
                } else {
                    ui.add_space(18.0);
                }
                if let Some((settings, _)) = marks {
                    let (icon, color) = foldericons::mark(settings, &folder.key);
                    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                    let color = color.and_then(foldericons::color).unwrap_or(ui.visuals().weak_text_color());
                    foldericons::paint(ui.painter(), rect, icon.unwrap_or_else(|| foldericons::guess(&folder.name)), color);
                }
                let label = folder.count.map_or_else(|| folder.name.clone(), |n| format!("{}  ({n})", folder.name));
                let response = ui.selectable_label(current.as_ref() == Some(&folder.key), label);
                if response.clicked() {
                    done = Some(Done::Chosen(folder.path.clone()));
                }
                if let Some((_, lang)) = marks {
                    response.context_menu(|ui| {
                        if let Some(chosen) = menu(ui, folder, lang) {
                            done = Some(chosen);
                            ui.close();
                        }
                    });
                }
            });
        }
    });
    done
}

fn menu(ui: &mut egui::Ui, folder: &Folder, lang: Language) -> Option<Done> {
    let mut done = None;
    ui.menu_button(Text::FolderColor.get(lang), |ui| {
        for (hex, text) in foldericons::COLORS {
            let color = foldericons::color(hex).unwrap_or_default();
            if ui.button(egui::RichText::new(text.get(lang)).color(color)).clicked() {
                done = Some(Done::Mark(folder.path.clone(), "color", Some(hex)));
            }
        }
        if ui.button(Text::FolderNoColor.get(lang)).clicked() {
            done = Some(Done::Mark(folder.path.clone(), "color", None));
        }
    });
    ui.menu_button(Text::FolderIcon.get(lang), |ui| {
        if ui.button(Text::FolderIconAuto.get(lang)).clicked() {
            done = Some(Done::Mark(folder.path.clone(), "icon", None));
        }
        for (name, text, _) in foldericons::ICONS {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                foldericons::paint(ui.painter(), rect, name, ui.visuals().text_color());
                if ui.button(text.get(lang)).clicked() {
                    done = Some(Done::Mark(folder.path.clone(), "icon", Some(name)));
                }
            });
        }
    });
    if ui.button(Text::FolderReveal.get(lang)).clicked() {
        done = Some(Done::Reveal(folder.path.clone()));
    }
    if folder.parent.is_some() {
        ui.separator();
        if ui.button(Text::FolderHide.get(lang)).clicked() {
            done = Some(Done::Hide(folder.path.clone()));
        }
        let text = crate::texts::fill(Text::FolderHideName.get(lang), &[("name", &folder.name)]);
        if ui.button(text).clicked() {
            done = Some(Done::HideName(folder.name.clone()));
        }
    }
    done
}

/// The tree at the left of a tab: the favourites, everything, and the folders.
#[derive(Default)]
pub struct SideTree {
    pub folders: Vec<Folder>,
    pub expanded: HashSet<String>,
    /// The folder shown; `None` for everything.
    pub folder: Option<PathBuf>,
    /// The favourites are shown.
    pub favorites: bool,
}

/// What changed in the tree.
#[derive(Default)]
pub struct Changed {
    /// Another folder is shown.
    pub shown: bool,
    /// The settings: what is hidden, or the icons.
    pub settings: bool,
}

impl SideTree {
    /// Draws it. `counts`: how many favourites there are, and how many items.
    pub fn show(&mut self, ui: &mut egui::Ui, id: &str, settings: &mut Settings, lang: Language, view: TabView, counts: (usize, usize)) -> Changed {
        let mut out = Changed::default();
        if view.favorites {
            let text = format!("{}  ({})", Text::Favorites.get(lang), counts.0);
            if ui.selectable_label(self.favorites, text).clicked() {
                (self.favorites, self.folder, out.shown) = (true, None, true);
            }
        }
        if view.all {
            let text = format!("{}  ({})", Text::AllFolders.get(lang), counts.1);
            if ui.selectable_label(!self.favorites && self.folder.is_none(), text).clicked() {
                (self.favorites, self.folder, out.shown) = (false, None, true);
            }
        }
        match draw(ui, id, &self.folders, &mut self.expanded, "", self.folder.as_deref(), Some((settings, lang))) {
            None => return out,
            Some(Done::Chosen(path)) => {
                (self.favorites, self.folder, out.shown) = (false, Some(path), true);
                return out;
            }
            Some(Done::CloseAll) => {
                self.expanded.clear();
                return out;
            }
            Some(Done::Reveal(path)) => {
                crate::platform::reveal(&path);
                return out;
            }
            Some(Done::Mark(path, what, value)) => foldericons::set_mark(settings, &path, what, value),
            Some(Done::Hide(path)) => settings.hide_path(&path),
            Some(Done::HideName(name)) => settings.hide_name(&name),
        }
        out.settings = true;
        out
    }
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
    fn items_are_counted_up_to_their_root() {
        let items = ["S:\\Lib\\Chairs\\Oak", "S:\\Lib\\Chairs\\Oak", "S:\\Lib\\Chairs"].map(PathBuf::from);
        let totals = totals(&["S:\\Lib".into()], items.iter().map(PathBuf::as_path));
        let of = |path: &str| totals.get(&norm_key(Path::new(path))).copied();
        assert_eq!((of("S:\\Lib"), of("S:\\Lib\\Chairs"), of("S:\\Lib\\Chairs\\Oak"), of("S:\\")), (Some(3), Some(3), Some(2), None));
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
