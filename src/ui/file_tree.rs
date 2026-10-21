//! Folder tree / flat list of changed files, like Fork's file panes.

use std::collections::BTreeMap;

use egui::{Id, Response, Ui};

use super::widgets::{self, Row};
use super::{icons, split_last, theme};
use crate::git::FileChange;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ListMode {
    #[default]
    Tree,
    Flat,
}

pub enum Target<'a> {
    File(&'a FileChange),
    Folder { path: &'a str, files: Vec<&'a FileChange> },
}

pub enum Event {
    Select(usize),
    Activate(usize),
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: Vec<usize>,
}

impl Dir {
    fn build(files: &[FileChange]) -> Self {
        let mut root = Dir::default();
        for (i, f) in files.iter().enumerate() {
            let mut node = &mut root;
            let (dir, _) = split_last(&f.path);
            for part in dir.split('/').filter(|p| !p.is_empty()) {
                node = node.dirs.entry(part.to_owned()).or_default();
            }
            node.files.push(i);
        }
        root.compact();
        root
    }

    /// Merges single-child folder chains: `src` › `git` becomes `src/git`.
    fn compact(&mut self) {
        let dirs = std::mem::take(&mut self.dirs);
        for (mut name, mut dir) in dirs {
            while dir.files.is_empty() && dir.dirs.len() == 1 {
                let (child_name, child) = dir.dirs.pop_first().expect("one child");
                name = format!("{name}/{child_name}");
                dir = child;
            }
            dir.compact();
            self.dirs.insert(name, dir);
        }
    }

    fn all_files<'a>(&self, files: &'a [FileChange], out: &mut Vec<&'a FileChange>) {
        for d in self.dirs.values() {
            d.all_files(files, out);
        }
        out.extend(self.files.iter().map(|&i| &files[i]));
    }
}

pub fn show(
    ui: &mut Ui,
    id: Id,
    files: &[FileChange],
    mode: ListMode,
    selected: Option<usize>,
    menu: &mut dyn FnMut(&mut Ui, Target<'_>),
) -> Option<Event> {
    let mut event = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    match mode {
        ListMode::Flat => {
            for (i, f) in files.iter().enumerate() {
                let (dir, name) = split_last(&f.path);
                let resp = Row::new(name)
                    .status(f.kind)
                    .detail(dir.trim_end_matches('/'))
                    .selected(selected == Some(i))
                    .show(ui);
                file_events(resp, i, f, &mut event, menu);
            }
        }
        ListMode::Tree => {
            let root = Dir::build(files);
            dir_rows(ui, id, &root, "", 0, files, selected, &mut event, menu);
        }
    }
    event
}

fn file_events(
    resp: Response,
    i: usize,
    f: &FileChange,
    event: &mut Option<Event>,
    menu: &mut dyn FnMut(&mut Ui, Target<'_>),
) {
    let resp = resp.on_hover_text(&f.path);
    if resp.double_clicked() {
        *event = Some(Event::Activate(i));
    } else if resp.clicked() {
        *event = Some(Event::Select(i));
    }
    resp.context_menu(|ui| menu(ui, Target::File(f)));
}

#[allow(clippy::too_many_arguments)]
fn dir_rows(
    ui: &mut Ui,
    id: Id,
    dir: &Dir,
    prefix: &str,
    depth: usize,
    files: &[FileChange],
    selected: Option<usize>,
    event: &mut Option<Event>,
    menu: &mut dyn FnMut(&mut Ui, Target<'_>),
) {
    let p = theme::of(ui);
    for (name, child) in &dir.dirs {
        let path = format!("{prefix}{name}/");
        let folder_id = id.with(&path);
        let open = widgets::folder_open(ui, folder_id, true);
        let mut contained = Vec::new();
        child.all_files(files, &mut contained);
        let resp = Row::new(name)
            .depth(depth)
            .disclosure(open)
            .icon(if open { icons::FOLDER_OPEN } else { icons::FOLDER }, p.accent)
            .trailing(contained.len().to_string())
            .show(ui);
        if resp.clicked() {
            widgets::toggle_folder(ui, folder_id);
        }
        resp.context_menu(|ui| {
            menu(
                ui,
                Target::Folder {
                    path: &path,
                    files: contained.clone(),
                },
            )
        });
        if open {
            dir_rows(ui, id, child, &path, depth + 1, files, selected, event, menu);
        }
    }
    for &i in &dir.files {
        let f = &files[i];
        let (_, name) = split_last(&f.path);
        let resp = Row::new(name)
            .depth(depth)
            .status(f.kind)
            .selected(selected == Some(i))
            .show(ui);
        file_events(resp, i, f, event, menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::ChangeKind;

    fn change(path: &str) -> FileChange {
        FileChange {
            path: path.into(),
            old_path: None,
            kind: ChangeKind::Modified,
        }
    }

    #[test]
    fn single_child_folders_are_compacted() {
        let files = [
            change("src/git/a.rs"),
            change("src/git/b.rs"),
            change("README.md"),
        ];
        let root = Dir::build(&files);
        assert_eq!(root.dirs.keys().collect::<Vec<_>>(), ["src/git"]);
        assert_eq!(root.files, vec![2]);
        assert_eq!(root.dirs["src/git"].files, vec![0, 1]);
    }

    #[test]
    fn folders_with_files_are_not_compacted() {
        let files = [change("src/main.rs"), change("src/ui/a.rs")];
        let root = Dir::build(&files);
        let src = &root.dirs["src"];
        assert_eq!(src.files, vec![0]);
        assert!(src.dirs.contains_key("ui"));
    }
}
