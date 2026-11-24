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

/// How many items are in each folder and the folders in it, from the folder of each item as [`norm_key`] gives it.
pub fn totals<'a>(roots: &[PathBuf], items: impl Iterator<Item = &'a str>) -> HashMap<String, usize> {
    let roots: HashSet<String> = roots.iter().map(|r| norm_key(r)).collect();
    let mut direct: HashMap<&str, usize> = HashMap::new();
    for folder in items {
        *direct.entry(folder).or_default() += 1;
    }
    let mut out = HashMap::new();
    for (folder, count) in direct {
        for at in Path::new(folder).ancestors() {
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
    OpenInside(PathBuf),
    Unhide(Option<Hidden>),
    Action(TreeAction),
}

/// A hidden folder: all of a name, or one.
#[derive(Clone)]
enum Hidden {
    Name(String),
    Path(PathBuf),
}

/// What the window does for the tree.
#[derive(Clone, Debug, PartialEq)]
pub enum TreeAction {
    /// Look for changes in the folders.
    Rescan,
    /// Write the names to hide.
    HideNames,
    /// A new folder in this one.
    NewFolder(PathBuf),
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
    let height = ui.spacing().interact_size.y.max(26.0);
    let p = crate::kit::pal(ui);
    let current = chosen.map(norm_key);
    let mut done = None;
    egui::ScrollArea::both().id_salt(id).auto_shrink(false).show_rows(ui, height, list.len(), |ui, range| {
        for &i in &list[range] {
            let folder = &folders[i];
            ui.horizontal(|ui| {
                ui.set_height(height);
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.add_space(folder.depth as f32 * 14.0);
                let open = expanded.contains(&folder.key);
                if folder.kids {
                    let (square, arrow) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::click());
                    let color = if arrow.hovered() { p.text } else { p.faint };
                    crate::kit::paint_icon(ui.painter(), square.shrink(3.0), if open { "down" } else { "right" }, color);
                    if arrow.clicked() {
                        if ui.input(|i| i.modifiers.shift) {
                            done = Some(Done::CloseAll);
                        } else if open {
                            expanded.remove(&folder.key);
                        } else {
                            expanded.insert(folder.key.clone());
                        }
                    }
                } else {
                    ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
                }
                if let Some((settings, _)) = marks {
                    let (icon, color) = foldericons::mark(settings, &folder.key);
                    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::hover());
                    let color = color.and_then(foldericons::color).unwrap_or(ui.visuals().weak_text_color());
                    foldericons::paint(ui.painter(), rect, icon.unwrap_or_else(|| foldericons::guess(&folder.name)), color);
                }
                let on = current.as_ref() == Some(&folder.key);
                let mut label = egui::text::LayoutJob::default();
                let format = |size: f32, color| egui::TextFormat::simple(egui::FontId::proportional(size), color);
                label.append(&folder.name, 0.0, format(13.5, if on { p.accent } else { p.text }));
                if let Some(count) = folder.count {
                    label.append(&count.to_string(), 8.0, format(11.5, p.faint));
                }
                let response = ui.selectable_label(on, label);
                if response.double_clicked() && folder.kids {
                    if open {
                        expanded.remove(&folder.key)
                    } else {
                        expanded.insert(folder.key.clone())
                    };
                }
                if response.clicked() {
                    done = Some(Done::Chosen(folder.path.clone()));
                }
                if let Some((settings, lang)) = marks {
                    response.context_menu(|ui| {
                        if let Some(chosen) = menu(ui, folder, settings, lang) {
                            done = Some(chosen);
                            ui.close();
                        }
                    });
                }
            });
        }
        // the menu of the tree itself, where there are no folders
        let rest = ui.available_rect_before_wrap();
        if let Some((settings, lang)) = marks
            && rest.height() > 8.0
        {
            ui.interact(rest, ui.scope_id().with("tree-rest"), egui::Sense::click()).context_menu(|ui| {
                if let Some(chosen) = common_menu(ui, settings, lang) {
                    done = Some(chosen);
                    ui.close();
                }
            });
        }
    });
    done
}

/// What the menu has wherever it is opened.
fn common_menu(ui: &mut egui::Ui, settings: &Settings, lang: Language) -> Option<Done> {
    let mut done = None;
    let (names, paths) = settings.hidden();
    if !names.is_empty() || !paths.is_empty() {
        let text = crate::texts::fill(Text::FoldersHidden.get(lang), &[("count", &(names.len() + paths.len()))]);
        ui.menu_button(text, |ui| {
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for name in names {
                    if ui.button(crate::texts::fill(Text::FolderShowName.get(lang), &[("name", &name)])).clicked() {
                        done = Some(Done::Unhide(Some(Hidden::Name(name))));
                    }
                }
                for path in paths {
                    let text = crate::texts::fill(Text::FolderShowAgain.get(lang), &[("name", &file_name(&path))]);
                    if ui.button(text).on_hover_text(path.display().to_string()).clicked() {
                        done = Some(Done::Unhide(Some(Hidden::Path(path))));
                    }
                }
            });
            ui.separator();
            if ui.button(Text::FolderShowAll.get(lang)).clicked() {
                done = Some(Done::Unhide(None));
            }
        });
    }
    if ui.button(Text::HideNames.get(lang)).clicked() {
        done = Some(Done::Action(TreeAction::HideNames));
    }
    ui.separator();
    if ui.button(Text::RefreshQuick.get(lang)).clicked() {
        done = Some(Done::Action(TreeAction::Rescan));
    }
    done
}

fn menu(ui: &mut egui::Ui, folder: &Folder, settings: &Settings, lang: Language) -> Option<Done> {
    ui.set_min_width(240.0);
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
    if ui.button(Text::CatalogNewFolder.get(lang)).clicked() {
        done = Some(Done::Action(TreeAction::NewFolder(folder.path.clone())));
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
    ui.separator();
    if folder.kids && ui.button(Text::FolderOpenInside.get(lang)).clicked() {
        done = Some(Done::OpenInside(folder.path.clone()));
    }
    common_menu(ui, settings, lang).or(done)
}

/// A row above the folders.
fn special(ui: &mut egui::Ui, on: bool, name: &str, count: usize) -> bool {
    let p = crate::kit::pal(ui);
    let mut label = egui::text::LayoutJob::default();
    let format = |font, color| egui::TextFormat::simple(font, color);
    label.append(name, 0.0, format(crate::kit::strong(13.5), if on { p.accent } else { p.text }));
    label.append(&count.to_string(), 8.0, format(egui::FontId::proportional(11.5), p.faint));
    ui.selectable_label(on, label).clicked()
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
    pub action: Option<TreeAction>,
}

impl SideTree {
    /// Draws it. `counts`: how many favourites there are, and how many items.
    pub fn show(&mut self, ui: &mut egui::Ui, id: &str, settings: &mut Settings, lang: Language, view: TabView, counts: (usize, usize)) -> Changed {
        let mut out = Changed::default();
        if view.favorites && special(ui, self.favorites, Text::Favorites.get(lang), counts.0) {
            (self.favorites, self.folder, out.shown) = (true, None, true);
        }
        if view.all && special(ui, !self.favorites && self.folder.is_none(), Text::AllFolders.get(lang), counts.1) {
            (self.favorites, self.folder, out.shown) = (false, None, true);
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
            Some(Done::OpenInside(path)) => {
                let key = norm_key(&path);
                let prefix = format!("{}\\", key.trim_end_matches('\\'));
                let inside = |k: &String| *k == key || k.starts_with(&prefix);
                self.expanded.extend(self.folders.iter().filter(|f| f.kids).map(|f| f.key.clone()).filter(inside));
                return out;
            }
            Some(Done::Action(action)) => {
                out.action = Some(action);
                return out;
            }
            Some(Done::Unhide(None)) => settings.show_hidden(None, None),
            Some(Done::Unhide(Some(Hidden::Name(name)))) => settings.show_hidden(Some(&name), None),
            Some(Done::Unhide(Some(Hidden::Path(path)))) => settings.show_hidden(None, Some(&path)),
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
        let items = ["S:\\Lib\\Chairs\\Oak", "S:\\Lib\\Chairs\\Oak", "S:\\Lib\\Chairs"].map(|f| norm_key(Path::new(f)));
        let totals = totals(&["S:\\Lib".into()], items.iter().map(String::as_str));
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
