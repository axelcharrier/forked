use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use egui::{
    Align2, Button, CentralPanel, Color32, Context, FontId, Frame, Id, Key, KeyboardShortcut,
    Margin, Modifiers, Panel, Rect, RichText, Sense, Stroke, StrokeKind, Ui, ViewportCommand,
    pos2, vec2,
};

use crate::git::cli;
use crate::git::patch::PatchMode;
use crate::state::AppState;
use crate::tab::RepoTab;
use crate::ui::dialogs::{self, Outcome};
use crate::ui::widgets::{self, Row};
use crate::ui::{
    Action, After, Dialog, GitOp, View, changes, history, icons, sidebar, theme, toolbar,
};

struct Task {
    repo: Option<PathBuf>,
    op: GitOp,
    rx: Receiver<Result<String, String>>,
}

struct Toast {
    text: String,
    error: bool,
    until: f64,
}

pub struct ForkedApp {
    tabs: Vec<RepoTab>,
    active: usize,
    state: AppState,
    dialog: Option<Dialog>,
    running: Vec<Task>,
    queue: VecDeque<(Option<PathBuf>, GitOp)>,
    toasts: Vec<Toast>,
    focused: bool,
    /// The "new tab" page is showing instead of a repository.
    welcome_tab: bool,
    debug: Option<crate::debug_shot::DebugShot>,
}

impl ForkedApp {
    pub fn new(cc: &eframe::CreationContext<'_>, open_arg: Option<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            state: AppState::load(),
            dialog: None,
            running: Vec::new(),
            queue: VecDeque::new(),
            toasts: Vec::new(),
            focused: true,
            welcome_tab: false,
            debug: crate::debug_shot::DebugShot::from_env(),
        };
        for path in app.state.open.clone() {
            if let Ok(tab) = RepoTab::open(&path) {
                app.tabs.push(tab);
            }
        }
        app.active = app.state.active.min(app.tabs.len().saturating_sub(1));
        if let Some(path) = open_arg {
            app.open_repo(&path, 0.0);
        }
        app
    }

    fn save_state(&mut self) {
        self.state.open = self.tabs.iter().map(|t| t.path().clone()).collect();
        self.state.active = self.active;
        self.state.save();
    }

    fn toast(&mut self, now: f64, text: impl Into<String>, error: bool) {
        let ttl = if error { 12.0 } else { 3.5 };
        self.toasts.push(Toast {
            text: text.into(),
            error,
            until: now + ttl,
        });
    }

    fn open_repo(&mut self, path: &Path, now: f64) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| canonical.starts_with(t.path()))
        {
            self.active = i;
            self.welcome_tab = false;
            self.save_state();
            return;
        }
        match RepoTab::open(path) {
            Ok(tab) => {
                self.welcome_tab = false;
                self.state.add_recent(tab.path().clone());
                self.tabs.push(tab);
                self.active = self.tabs.len() - 1;
                self.save_state();
            }
            Err(e) => self.toast(now, format!("{e:#}"), true),
        }
    }

    fn active_tab(&mut self) -> Option<&mut RepoTab> {
        self.tabs.get_mut(self.active)
    }

    /// Queues `op`; operations on the same repository run one at a time.
    fn run(&mut self, ctx: &Context, op: GitOp) {
        let repo = if op.cwd.is_some() {
            None
        } else {
            self.tabs.get(self.active).map(|t| t.path().clone())
        };
        self.queue.push_back((repo, op));
        self.start_queued(ctx);
    }

    fn start_queued(&mut self, ctx: &Context) {
        let mut i = 0;
        while i < self.queue.len() {
            let repo = &self.queue[i].0;
            let blocked = repo.is_some() && self.running.iter().any(|t| t.repo == *repo);
            if blocked {
                i += 1;
                continue;
            }
            let (repo, op) = self.queue.remove(i).expect("index in range");
            let Some(cwd) = op.cwd.clone().or(repo.clone()) else {
                continue;
            };
            let (tx, rx) = mpsc::channel();
            let steps = op.steps.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let mut result = Ok(String::new());
                for step in steps {
                    match cli::run(&cwd, &step.args, step.stdin.as_deref()) {
                        Ok(out) => result = Ok(out),
                        Err(e) => {
                            result = Err(format!("{e:#}"));
                            break;
                        }
                    }
                }
                let _ = tx.send(result);
                ctx.request_repaint();
            });
            self.running.push(Task { repo, op, rx });
        }
    }

    fn poll_tasks(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        let mut done = Vec::new();
        self.running.retain(|t| match t.rx.try_recv() {
            Ok(r) => {
                done.push((t.repo.clone(), t.op.clone(), r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                done.push((t.repo.clone(), t.op.clone(), Err("opération interrompue".into())));
                false
            }
        });
        for (repo, op, result) in done {
            match result {
                Ok(_) => {
                    if !op.quiet {
                        self.toast(now, format!("✔ {}", op.label), false);
                    }
                    match &op.after {
                        After::Nothing => {}
                        After::ClearCommitMessage => {
                            if let Some(tab) = self.tabs.iter_mut().find(|t| Some(t.path()) == repo.as_ref()) {
                                tab.clear_message();
                            }
                        }
                        After::OpenRepo(path) => self.open_repo(path, now),
                    }
                }
                Err(e) => self.toast(now, format!("{} : {e}", op.label), true),
            }
            if let Some(tab) = self.tabs.iter_mut().find(|t| Some(t.path()) == repo.as_ref()) {
                tab.refresh();
            }
        }
        self.start_queued(ctx);
    }

    fn busy_labels(&self, repo: &Path) -> Vec<String> {
        self.running
            .iter()
            .map(|t| (&t.repo, &t.op))
            .chain(self.queue.iter().map(|(r, o)| (r, o)))
            .filter(|(r, _)| r.as_deref() == Some(repo))
            .map(|(_, o)| o.label.clone())
            .collect()
    }

    fn handle_global_input(&mut self, ctx: &Context, actions: &mut Vec<Action>) {
        // Refresh when coming back to the app, as files may have changed outside.
        let focused = ctx.input(|i| i.focused);
        if focused && !self.focused {
            actions.push(Action::Refresh);
        }
        self.focused = focused;

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        for p in dropped {
            actions.push(Action::OpenRepoAt(p));
        }

        if self.dialog.is_some() {
            return;
        }
        let shortcut = |key| KeyboardShortcut::new(Modifiers::COMMAND, key);
        ctx.input_mut(|i| {
            if i.consume_shortcut(&shortcut(Key::O)) {
                actions.push(Action::OpenRepo);
            }
            if i.consume_shortcut(&shortcut(Key::R)) {
                actions.push(Action::Refresh);
            }
            if i.consume_shortcut(&shortcut(Key::T)) {
                actions.push(Action::NewTab);
            }
            if i.consume_shortcut(&shortcut(Key::W)) {
                if self.welcome_tab && !self.tabs.is_empty() {
                    actions.push(Action::SwitchTab(self.active));
                } else if !self.tabs.is_empty() {
                    actions.push(Action::CloseTab(self.active));
                }
            }
            if i.consume_shortcut(&shortcut(Key::Num1)) {
                actions.push(Action::SetView(View::Changes));
            }
            if i.consume_shortcut(&shortcut(Key::Num2)) {
                actions.push(Action::SetView(View::History));
            }
            if i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::Tab)) && !self.tabs.is_empty() {
                actions.push(Action::SwitchTab((self.active + 1) % self.tabs.len()));
            }
        });
    }

    fn apply(&mut self, ctx: &Context, action: Action) {
        let now = ctx.input(|i| i.time);
        match action {
            Action::Git(op) => self.run(ctx, op),
            Action::Confirm { title, message, op } => {
                self.dialog = Some(Dialog::Confirm { title, message, op });
            }
            Action::Dialog(d) => self.dialog = Some(d),
            Action::Refresh => {
                if let Some(tab) = self.active_tab() {
                    tab.refresh();
                }
            }
            Action::SetView(v) => {
                if let Some(tab) = self.active_tab() {
                    tab.view = v;
                }
            }
            Action::SelectCommit(oid) => {
                if let Some(tab) = self.active_tab() {
                    tab.select_commit(oid);
                }
            }
            Action::RevealCommit(oid) => {
                if let Some(tab) = self.active_tab() {
                    tab.view = View::History;
                    tab.search.clear();
                    tab.scroll_to = tab.index_of.get(&oid).copied();
                    tab.select_commit(oid);
                }
            }
            Action::ShowCommit(oid) => {
                if let Some(tab) = self.active_tab() {
                    tab.view = View::History;
                    tab.select_commit(oid);
                }
            }
            Action::SelectCommitFile(i) => {
                if let Some(tab) = self.active_tab() {
                    tab.select_commit_file(i);
                }
            }
            Action::SelectChange(side, path) => {
                if let Some(tab) = self.active_tab() {
                    tab.select_change(side, path);
                }
            }
            Action::Patch { mode, hunk, lines } => {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return;
                };
                let Some(op) = tab.patch_op(mode, hunk, lines.as_deref()) else {
                    return;
                };
                tab.selected_lines.clear();
                tab.line_anchor = None;
                if mode == PatchMode::Discard {
                    self.dialog = Some(Dialog::Confirm {
                        title: "Annuler des modifications".into(),
                        message: "Ces modifications seront définitivement perdues.".into(),
                        op,
                    });
                } else {
                    self.run(ctx, op.quiet());
                }
            }
            Action::Commit => {
                if let Some(op) = self.tabs.get(self.active).and_then(RepoTab::commit_op) {
                    self.run(ctx, op);
                }
            }
            Action::Ignore(path) => {
                if let Some(tab) = self.active_tab() {
                    let file = tab.path().join(".gitignore");
                    let mut content = std::fs::read_to_string(&file).unwrap_or_default();
                    if !content.is_empty() && !content.ends_with('\n') {
                        content.push('\n');
                    }
                    content.push_str(&format!("/{path}\n"));
                    let result = std::fs::write(&file, content);
                    tab.refresh();
                    if let Err(e) = result {
                        self.toast(now, format!(".gitignore : {e}"), true);
                    }
                }
            }
            Action::Copy(text) => {
                ctx.copy_text(text);
                self.toast(now, "Copié", false);
            }
            Action::Open(path) => {
                let _ = Command::new("open").arg(path).spawn();
            }
            Action::Reveal(path) => {
                let _ = Command::new("open").arg("-R").arg(path).spawn();
            }
            Action::OpenTerminal => {
                if let Some(tab) = self.tabs.get(self.active) {
                    let _ = Command::new("open")
                        .args(["-a", "Terminal"])
                        .arg(tab.path())
                        .spawn();
                }
            }
            Action::OpenRepo => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Ouvrir un dépôt Git")
                    .pick_folder()
                {
                    self.open_repo(&dir, now);
                }
            }
            Action::OpenRepoAt(path) => self.open_repo(&path, now),
            Action::InitRepo => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Créer un dépôt Git dans ce dossier")
                    .pick_folder()
                {
                    let op = GitOp::new("git init", ["init"])
                        .cwd(dir.clone())
                        .after(After::OpenRepo(dir));
                    self.run(ctx, op);
                }
            }
            Action::CloneRepo => {
                let parent = dirs::home_dir()
                    .map(|h| h.join("Development"))
                    .filter(|p| p.is_dir())
                    .or_else(dirs::home_dir)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.dialog = Some(Dialog::Clone {
                    url: String::new(),
                    parent,
                    name: String::new(),
                });
            }
            Action::NewTab => self.welcome_tab = true,
            Action::SwitchTab(i) => {
                self.welcome_tab = false;
                if i < self.tabs.len() {
                    self.active = i;
                    self.tabs[i].refresh();
                    self.save_state();
                }
            }
            Action::CloseTab(i) => {
                if i < self.tabs.len() {
                    self.tabs.remove(i);
                    if self.active >= i && self.active > 0 {
                        self.active -= 1;
                    }
                    self.save_state();
                }
            }
        }
    }

    fn tab_bar(&self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let p = theme::of(ui);
        let full = ui.max_rect();
        // The tab strip doubles as the window's title bar.
        let bar = ui.interact(full, Id::unique("titlebar"), Sense::click_and_drag());
        if bar.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if bar.double_clicked() {
            let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }

        let left = full.left() + 84.0; // room for the traffic lights
        let count = self.tabs.len() + usize::from(self.welcome_tab || self.tabs.is_empty());
        let avail = full.right() - left - 48.0;
        let tab_w = (avail / count.max(1) as f32).clamp(120.0, 240.0);
        let painter = ui.painter().clone();
        let mut x = left;
        let mut draw_tab = |ui: &mut Ui, id: Id, label: &str, icon: &str, selected: bool, closable: bool| {
            let rect = Rect::from_min_size(pos2(x, full.top() + 6.0), vec2(tab_w - 4.0, full.height() - 12.0));
            x += tab_w;
            let resp = ui.interact(rect, id, Sense::click());
            let fill = if selected {
                p.content
            } else if resp.hovered() {
                p.hover
            } else {
                Color32::TRANSPARENT
            };
            painter.rect_filled(rect, 7.0, fill);
            if selected {
                painter.rect_stroke(rect, 7.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
            }
            let color = if selected { p.text } else { p.weak };
            let clip = painter.with_clip_rect(rect.shrink2(vec2(26.0, 0.0)));
            let galley = painter.layout_no_wrap(format!("{icon}  {label}"), FontId::proportional(13.0), color);
            let pos = pos2(
                (rect.center().x - galley.size().x / 2.0).max(rect.left() + 26.0),
                rect.center().y - galley.size().y / 2.0,
            );
            clip.galley(pos, galley, color);
            let mut closed = false;
            if closable && (resp.hovered() || selected) {
                let close = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(16.0, 16.0));
                let close_resp = ui.interact(close, id.with("close"), Sense::click());
                if close_resp.hovered() {
                    painter.rect_filled(close, 4.0, p.hover);
                }
                painter.text(close.center(), Align2::CENTER_CENTER, icons::CLOSE, FontId::proportional(10.0), p.weak);
                closed = close_resp.clicked();
            }
            (resp, closed)
        };

        for (i, t) in self.tabs.iter().enumerate() {
            let selected = i == self.active && !self.welcome_tab;
            let (resp, closed) = draw_tab(ui, Id::unique(("tab", i)), &t.name, icons::FOLDER_SIMPLE, selected, true);
            if closed || resp.middle_clicked() {
                actions.push(Action::CloseTab(i));
            } else if resp.clicked() {
                actions.push(Action::SwitchTab(i));
            }
            resp.on_hover_text(t.path().display().to_string());
        }
        if self.welcome_tab || self.tabs.is_empty() {
            let (_, closed) = draw_tab(ui, Id::unique("welcome_tab"), "Nouvel onglet", icons::PLUS, true, !self.tabs.is_empty());
            if closed {
                actions.push(Action::SwitchTab(self.active));
            }
        }
        let plus = Rect::from_center_size(pos2(x + 16.0, full.center().y), vec2(26.0, 24.0));
        let plus_resp = ui.interact(plus, Id::unique("new_tab"), Sense::click());
        if plus_resp.hovered() {
            painter.rect_filled(plus, 6.0, p.hover);
        }
        painter.text(plus.center(), Align2::CENTER_CENTER, icons::PLUS, FontId::proportional(15.0), p.weak);
        if plus_resp.on_hover_text("Nouvel onglet (⌘T)").clicked() {
            actions.push(Action::NewTab);
        }
    }

    fn welcome(&self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let p = theme::of(ui);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.12).max(20.0));
            ui.label(RichText::new(icons::FORK).size(56.0).color(p.accent));
            ui.add_space(4.0);
            ui.label(RichText::new("Forked").size(30.0).strong().color(p.text));
            ui.label(RichText::new("Client Git pour macOS").color(p.weak));
            ui.add_space(28.0);
            ui.allocate_ui(vec2(320.0, 400.0), |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let big = |icon: &str, text: &str| {
                    Button::new(RichText::new(format!("{icon}   {text}")).size(14.0)).min_size(vec2(320.0, 36.0))
                };
                if ui.add(big(icons::FOLDER_OPEN, "Ouvrir un dépôt…")).clicked() {
                    actions.push(Action::OpenRepo);
                }
                if ui.add(big(icons::FETCH, "Cloner un dépôt…")).clicked() {
                    actions.push(Action::CloneRepo);
                }
                if ui.add(big(icons::PLUS, "Créer un dépôt…")).clicked() {
                    actions.push(Action::InitRepo);
                }
                let recent: Vec<&PathBuf> = self.state.recent.iter().filter(|p| p.is_dir()).collect();
                if !recent.is_empty() {
                    ui.add_space(16.0);
                    ui.label(RichText::new("RÉCENTS").size(10.5).color(p.weak));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for path in recent {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let parent = path
                            .parent()
                            .map(|d| d.display().to_string().replace(&dirs::home_dir().unwrap_or_default().display().to_string(), "~"))
                            .unwrap_or_default();
                        if Row::new(&name)
                            .icon(icons::FOLDER_SIMPLE, p.accent)
                            .detail(&parent)
                            .show(ui)
                            .clicked()
                        {
                            actions.push(Action::OpenRepoAt(path.clone()));
                        }
                    }
                }
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Astuce : glissez un dossier dans la fenêtre pour l'ouvrir").color(p.weak).size(12.0));
        });
    }

    fn show_toasts(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| t.until > now);
        if self.toasts.is_empty() {
            return;
        }
        let mut dismiss = None;
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, egui::vec2(-14.0, -14.0))
            .show(ctx, |ui| {
                for (i, t) in self.toasts.iter().enumerate() {
                    let fill = if t.error {
                        Color32::from_rgb(0x9c, 0x2b, 0x2b)
                    } else {
                        Color32::from_rgb(0x2b, 0x6a, 0x3c)
                    };
                    let resp = Frame::popup(ui.style())
                        .fill(fill)
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(RichText::new(&t.text).color(Color32::WHITE));
                        })
                        .response
                        .interact(egui::Sense::click());
                    if resp.clicked() {
                        dismiss = Some(i);
                    }
                }
            });
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

impl eframe::App for ForkedApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(debug) = &mut self.debug
            && debug.tick(&ctx)
        {
            let view = debug.view.clone();
            if let Some(tab) = self.tabs.get_mut(self.active) {
                match view.as_deref() {
                    Some("history") => {
                        tab.view = View::History;
                        if let Some(c) = tab.commits.first() {
                            tab.select_commit(c.id);
                        }
                    }
                    Some("changes") => {
                        tab.view = View::Changes;
                        if let Some(f) = tab.status.unstaged.first() {
                            tab.select_change(crate::git::DiffSide::Unstaged, f.path.clone());
                        }
                    }
                    Some(v) if v.starts_with("checkout:") => {
                        self.dialog = crate::ui::checkout::dialog(tab, &v["checkout:".len()..], false);
                    }
                    _ => {}
                }
            }
        }
        self.poll_tasks(&ctx);
        let mut actions = Vec::new();
        self.handle_global_input(&ctx, &mut actions);

        let p = theme::of(ui);
        Panel::top("tabs")
            .exact_size(40.0)
            .frame(Frame::new().fill(p.bar))
            .show(ui, |ui| self.tab_bar(ui, &mut actions));

        if self.tabs.is_empty() || self.welcome_tab {
            CentralPanel::default()
                .frame(Frame::new().fill(p.content))
                .show(ui, |ui| self.welcome(ui, &mut actions));
        } else {
            let busy = self.busy_labels(self.tabs[self.active].path());
            let tab = &mut self.tabs[self.active];
            Panel::top("toolbar")
                .frame(Frame::new().fill(p.bar).inner_margin(Margin::symmetric(0, 3)))
                .show(ui, |ui| toolbar::show(ui, tab, &busy, &mut actions));
            if let Some(err) = tab.error.clone() {
                Panel::top("tab_error")
                    .frame(Frame::new().fill(theme::kind_color(crate::git::ChangeKind::Deleted).gamma_multiply(0.25)).inner_margin(Margin::symmetric(12, 6)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}  {err}", icons::WARNING)).color(p.text));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if widgets::icon_button(ui, icons::CLOSE, false, "Masquer").clicked() {
                                    tab.error = None;
                                }
                            });
                        });
                    });
            }
            Panel::left("sidebar")
                .resizable(true)
                .default_size(250.0)
                .min_size(190.0)
                .frame(Frame::new().fill(p.sidebar))
                .show(ui, |ui| sidebar::show(ui, tab, &mut actions));
            CentralPanel::no_frame().show(ui, |ui| match tab.view {
                View::History => history::show(ui, tab, &mut actions),
                View::Changes => changes::show(ui, tab, &mut actions),
            });
        }

        if let Some(dialog) = &mut self.dialog {
            match dialogs::show(&ctx, dialog, self.tabs.get(self.active)) {
                Outcome::Open => {}
                Outcome::Close => self.dialog = None,
                Outcome::Run(op) => {
                    self.dialog = None;
                    actions.push(Action::Git(op));
                }
            }
        }

        for action in actions {
            self.apply(&ctx, action);
        }
        self.show_toasts(&ctx);
    }
}
