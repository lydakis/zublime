use collections::{HashMap, HashSet};
use editor::Editor;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, MouseDownEvent,
    Render, ScrollHandle, WeakEntity, Window, actions, rems,
};
use menu::{
    Cancel, Confirm, SecondaryConfirm, SelectChild, SelectFirst, SelectLast, SelectNext,
    SelectParent, SelectPrevious,
};
use project::{DirectoryItem, Project};
use std::path::{Path, PathBuf};
use ui::{
    Color, Disclosure, Divider, Icon, IconName, IconSize, Label, ListItem, ListItemSpacing,
    ScrollAxes, Scrollbars, WithScrollbar, prelude::*,
};
use util::{ResultExt, paths::compare_paths};
use workspace::{DirectoryBrowserState, DismissDecision, ModalView, OpenOptions, Workspace};

actions!(
    directory_browser,
    [
        /// Toggles the directory browser.
        Toggle
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(DirectoryBrowser::register).detach();
}

#[derive(Clone)]
struct DirectoryEntry {
    path: PathBuf,
    is_dir: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowKind {
    Parent,
    Directory,
    File,
}

#[derive(Clone)]
struct TreeRow {
    path: PathBuf,
    depth: usize,
    kind: RowKind,
    is_expanded: bool,
    label: SharedString,
}

pub struct DirectoryBrowser {
    workspace: WeakEntity<Workspace>,
    project: Entity<Project>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    root_path: PathBuf,
    expanded_dirs: HashSet<PathBuf>,
    selected_path: Option<PathBuf>,
    selected_index: usize,
    rows: Vec<TreeRow>,
    directory_cache: HashMap<PathBuf, Vec<DirectoryEntry>>,
    pending_listings: HashSet<PathBuf>,
}

impl DirectoryBrowser {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _: &mut Context<Workspace>,
    ) {
        workspace.register_action(|workspace, _: &Toggle, window, cx| {
            let state = workspace.directory_browser_state().clone();
            let project = workspace.project().clone();
            let active_directory = active_directory(workspace, cx);
            let workspace_handle = workspace.weak_handle();

            workspace.toggle_modal(window, cx, |window, cx| {
                DirectoryBrowser::new(
                    workspace_handle,
                    project,
                    state,
                    active_directory,
                    window,
                    cx,
                )
            });
        });
    }

    fn new(
        workspace: WeakEntity<Workspace>,
        project: Entity<Project>,
        state: DirectoryBrowserState,
        active_directory: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root_path = resolve_root_path(&state, active_directory);
        let focus_handle = cx.focus_handle();

        let mut browser = Self {
            workspace,
            project,
            focus_handle: focus_handle.clone(),
            scroll_handle: ScrollHandle::new(),
            root_path,
            expanded_dirs: state.expanded_dirs,
            selected_path: state.selected_path,
            selected_index: 0,
            rows: Vec::new(),
            directory_cache: HashMap::default(),
            pending_listings: HashSet::default(),
        };
        let root_path = browser.root_path.clone();
        browser.ensure_directory_listed(&root_path, window, cx);
        browser.ensure_expanded_directories_listed(window, cx);
        browser.refresh_rows(cx);
        window.focus(&focus_handle, cx);
        browser
    }

    fn ensure_directory_listed(
        &mut self,
        path: &PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.directory_cache.contains_key(path) || self.pending_listings.contains(path) {
            return;
        }

        self.pending_listings.insert(path.clone());
        let project = self.project.clone();
        let path_clone = path.clone();
        let path_string = path.to_string_lossy().to_string();

        cx.spawn_in(window, async move |this, cx| {
            let listing = project.update(cx, |project, cx| project.list_directory(path_string, cx));
            let listing = listing.await;
            this.update(cx, |browser, cx| {
                browser.pending_listings.remove(&path_clone);
                let Some(items) = listing.log_err() else {
                    browser.refresh_rows(cx);
                    return;
                };

                let mut entries: Vec<DirectoryEntry> = items
                    .into_iter()
                    .map(|item| DirectoryEntry::from_item(&path_clone, item))
                    .collect();
                entries.sort_by(|a, b| compare_paths((&a.path, !a.is_dir), (&b.path, !b.is_dir)));
                browser.directory_cache.insert(path_clone, entries);
                browser.refresh_rows(cx);
            })
            .log_err();
        })
        .detach();
    }

    fn ensure_expanded_directories_listed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root_path = self.root_path.clone();
        let expanded_dirs = self.expanded_dirs.clone();
        for path in expanded_dirs {
            if path.starts_with(&root_path) {
                self.ensure_directory_listed(&path, window, cx);
            }
        }
    }

    fn refresh_rows(&mut self, cx: &mut Context<Self>) {
        let mut rows = Vec::new();
        if let Some(parent) = self.root_path.parent() {
            rows.push(TreeRow::parent(parent.to_path_buf()));
        }

        let mut visited = HashSet::default();
        self.append_directory_rows(&self.root_path, 0, &mut rows, &mut visited);
        self.rows = rows;
        self.restore_selection(cx);
    }

    fn append_directory_rows(
        &self,
        directory: &PathBuf,
        depth: usize,
        rows: &mut Vec<TreeRow>,
        visited: &mut HashSet<PathBuf>,
    ) {
        if !visited.insert(directory.clone()) {
            return;
        }

        let Some(entries) = self.directory_cache.get(directory) else {
            return;
        };

        for entry in entries {
            let label = entry_label(&entry.path);
            let is_expanded = entry.is_dir && self.expanded_dirs.contains(&entry.path);
            let kind = if entry.is_dir {
                RowKind::Directory
            } else {
                RowKind::File
            };
            rows.push(TreeRow {
                path: entry.path.clone(),
                depth,
                kind,
                is_expanded,
                label,
            });

            if entry.is_dir && is_expanded {
                self.append_directory_rows(&entry.path, depth + 1, rows, visited);
            }
        }
    }

    fn restore_selection(&mut self, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            self.selected_index = 0;
            self.selected_path = None;
            cx.notify();
            return;
        }

        let mut target_index = self.selected_index.min(self.rows.len().saturating_sub(1));
        let mut found_selection = false;
        if let Some(selected_path) = &self.selected_path {
            if let Some(ix) = self.rows.iter().position(|row| row.path == *selected_path) {
                target_index = ix;
                found_selection = true;
            }
        }

        let previous_selected_path = self.selected_path.clone();
        self.set_selected_index(target_index, cx);
        if !found_selection {
            self.selected_path = previous_selected_path;
        }
    }

    fn set_selected_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            self.selected_index = 0;
            self.selected_path = None;
            cx.notify();
            return;
        }

        let index = index.min(self.rows.len().saturating_sub(1));
        self.selected_index = index;
        self.selected_path = Some(self.rows[index].path.clone());
        self.scroll_handle.scroll_to_item(index);
        cx.notify();
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let next = self.selected_index.saturating_add(1);
        self.set_selected_index(next, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let prev = self.selected_index.saturating_sub(1);
        self.set_selected_index(prev, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.set_selected_index(0, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len().saturating_sub(1);
        self.set_selected_index(last, cx);
    }

    fn select_child(&mut self, _: &SelectChild, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(self.selected_index).cloned() else {
            return;
        };

        match row.kind {
            RowKind::Directory => {
                if !self.expanded_dirs.contains(&row.path) {
                    self.expand_directory(row.path, window, cx);
                    return;
                }

                if let Some(child_index) = self.first_child_index(self.selected_index) {
                    self.set_selected_index(child_index, cx);
                }
            }
            RowKind::Parent => {
                self.set_root_path(row.path, Some(self.root_path.clone()), window, cx);
            }
            RowKind::File => {}
        }
    }

    fn select_parent(&mut self, _: &SelectParent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(self.selected_index).cloned() else {
            return;
        };

        if row.kind == RowKind::Directory && self.expanded_dirs.contains(&row.path) {
            self.collapse_directory(row.path, cx);
            return;
        }

        if let Some(parent_index) = self.parent_index(self.selected_index) {
            self.set_selected_index(parent_index, cx);
            return;
        }

        if let Some(parent) = self.root_path.parent() {
            self.set_root_path(
                parent.to_path_buf(),
                Some(self.root_path.clone()),
                window,
                cx,
            );
        }
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_selected(window, cx);
    }

    fn secondary_confirm(
        &mut self,
        _: &SecondaryConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_selected(window, cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn activate_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(self.selected_index).cloned() else {
            return;
        };

        match row.kind {
            RowKind::Parent => {
                self.set_root_path(row.path, Some(self.root_path.clone()), window, cx);
            }
            RowKind::Directory => {
                if self.expanded_dirs.contains(&row.path) {
                    self.collapse_directory(row.path, cx);
                } else {
                    self.expand_directory(row.path, window, cx);
                }
            }
            RowKind::File => {
                self.open_file(row.path, window, cx);
            }
        }
    }

    fn open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(self.selected_index).cloned() else {
            return;
        };

        match row.kind {
            RowKind::Parent => {
                self.set_root_path(row.path, Some(self.root_path.clone()), window, cx);
            }
            RowKind::Directory => {
                self.set_root_path(row.path, None, window, cx);
            }
            RowKind::File => {
                self.open_file(row.path, window, cx);
            }
        }
    }

    fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            workspace
                .open_abs_path(path, OpenOptions::default(), window, cx)
                .detach_and_log_err(cx);
        });
        cx.emit(DismissEvent);
    }

    fn expand_directory(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.expanded_dirs.insert(path.clone());
        self.ensure_directory_listed(&path, window, cx);
        self.refresh_rows(cx);
    }

    fn collapse_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.expanded_dirs.remove(&path);
        self.refresh_rows(cx);
    }

    fn set_root_path(
        &mut self,
        path: PathBuf,
        select_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.root_path = path;
        self.selected_path = select_path;
        self.selected_index = 0;
        let root_path = self.root_path.clone();
        self.ensure_directory_listed(&root_path, window, cx);
        self.ensure_expanded_directories_listed(window, cx);
        self.refresh_rows(cx);
    }

    fn parent_index(&self, start_index: usize) -> Option<usize> {
        let current_depth = self.rows.get(start_index)?.depth;
        if current_depth == 0 {
            return None;
        }
        (0..start_index)
            .rev()
            .find(|ix| self.rows[*ix].depth < current_depth)
    }

    fn first_child_index(&self, start_index: usize) -> Option<usize> {
        let current_depth = self.rows.get(start_index)?.depth;
        let next_index = start_index.saturating_add(1);
        let row = self.rows.get(next_index)?;
        if row.depth > current_depth {
            Some(next_index)
        } else {
            None
        }
    }

    fn row_start_slot(
        &self,
        row: &TreeRow,
        row_index: usize,
        handle: WeakEntity<DirectoryBrowser>,
    ) -> AnyElement {
        let icon = match row.kind {
            RowKind::Parent => Icon::new(IconName::ArrowUp)
                .size(IconSize::Small)
                .color(Color::Muted)
                .into_any_element(),
            RowKind::Directory => Icon::new(if row.is_expanded {
                IconName::FolderOpen
            } else {
                IconName::Folder
            })
            .size(IconSize::Small)
            .color(Color::Muted)
            .into_any_element(),
            RowKind::File => Icon::new(IconName::File)
                .size(IconSize::Small)
                .color(Color::Muted)
                .into_any_element(),
        };

        let disclosure = if row.kind == RowKind::Directory {
            let path = row.path.clone();
            let is_open = row.is_expanded;
            Some(
                Disclosure::new(format!("dir-toggle-{row_index}"), is_open)
                    .on_click(move |_, window, cx| {
                        handle
                            .update(cx, |browser, cx| {
                                if browser.expanded_dirs.contains(&path) {
                                    browser.collapse_directory(path.clone(), cx);
                                } else {
                                    browser.expand_directory(path.clone(), window, cx);
                                }
                            })
                            .log_err();
                    })
                    .into_any_element(),
            )
        } else {
            None
        };

        h_flex()
            .gap_1()
            .child(
                disclosure.unwrap_or_else(|| div().size(IconSize::Small.rems()).into_any_element()),
            )
            .child(icon)
            .into_any_element()
    }

    fn render_row(
        &self,
        row: &TreeRow,
        row_index: usize,
        handle: WeakEntity<DirectoryBrowser>,
    ) -> ListItem {
        let selected = row_index == self.selected_index;

        ListItem::new(row_index)
            .spacing(ListItemSpacing::ExtraDense)
            .indent_level(row.depth)
            .inset(true)
            .start_slot(self.row_start_slot(row, row_index, handle.clone()))
            .toggle_state(selected)
            .on_click(move |event, window, cx| {
                handle
                    .update(cx, |browser, cx| {
                        browser.set_selected_index(row_index, cx);
                        if event.click_count() > 1 {
                            browser.open_selected(window, cx);
                        }
                    })
                    .log_err();
            })
            .child(Label::new(row.label.clone()))
    }

    fn persist_state(&mut self, cx: &mut Context<Self>) {
        let state = DirectoryBrowserState {
            root_path: Some(self.root_path.clone()),
            expanded_dirs: self.expanded_dirs.clone(),
            selected_path: self.selected_path.clone(),
        };
        let workspace = self.workspace.clone();
        cx.defer(move |cx| {
            let Some(workspace) = workspace.upgrade() else {
                return;
            };
            workspace.update(cx, |workspace, _| {
                workspace.set_directory_browser_state(state);
            });
        });
    }
}

impl Focusable for DirectoryBrowser {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for DirectoryBrowser {}

impl ModalView for DirectoryBrowser {
    fn on_before_dismiss(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DismissDecision {
        self.persist_state(cx);
        DismissDecision::Dismiss(true)
    }
}

impl Render for DirectoryBrowser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let handle = cx.entity().downgrade();
        let root_label = path_label(&self.root_path);

        let list = v_flex()
            .id("directory-browser-list")
            .flex_grow()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .children(self.rows.iter().enumerate().map(|(row_index, row)| {
                self.render_row(row, row_index, handle.clone())
                    .into_any_element()
            }))
            .custom_scrollbars(
                Scrollbars::new(ScrollAxes::Vertical).tracked_scroll_handle(&self.scroll_handle),
                window,
                cx,
            );

        v_flex()
            .key_context("menu")
            .w(rems(34.))
            .max_h(vh(0.7, window))
            .elevation_3(cx)
            .track_focus(&self.focus_handle)
            .on_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                this.focus_handle.focus(window, cx);
            }))
            .overflow_hidden()
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::select_child))
            .on_action(cx.listener(Self::select_parent))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::secondary_confirm))
            .on_action(cx.listener(Self::cancel))
            .child(
                v_flex()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .child(Label::new("Browse Files").color(Color::Muted))
                    .child(Label::new(root_label)),
            )
            .child(Divider::horizontal())
            .child(list)
    }
}

fn resolve_root_path(state: &DirectoryBrowserState, active_directory: Option<PathBuf>) -> PathBuf {
    if let Some(root) = &state.root_path {
        return root.clone();
    }

    if let Some(active) = active_directory {
        return active;
    }

    std::env::home_dir().unwrap_or_else(|| {
        if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        }
    })
}

fn active_directory(workspace: &Workspace, cx: &mut App) -> Option<PathBuf> {
    let active_item = workspace.active_item(cx)?;
    if let Some(project_path) = active_item.project_path(cx) {
        let project = workspace.project().read(cx);
        let abs_path = project.absolute_path(&project_path, cx)?;
        return abs_path.parent().map(|parent| parent.to_path_buf());
    }

    active_item.downcast::<Editor>().and_then(|editor| {
        editor.update(cx, |editor, cx| {
            editor
                .target_file_abs_path(cx)
                .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
        })
    })
}

fn entry_label(path: &Path) -> SharedString {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned().into())
        .unwrap_or_else(|| path.to_string_lossy().into_owned().into())
}

fn path_label(path: &Path) -> SharedString {
    path.to_string_lossy().into_owned().into()
}

impl DirectoryEntry {
    fn from_item(base: &PathBuf, item: DirectoryItem) -> Self {
        let path = if item.path.is_absolute() {
            item.path
        } else {
            base.join(item.path)
        };
        Self {
            path,
            is_dir: item.is_dir,
        }
    }
}

impl TreeRow {
    fn parent(path: PathBuf) -> Self {
        TreeRow {
            path,
            depth: 0,
            kind: RowKind::Parent,
            is_expanded: false,
            label: "Parent Folder".into(),
        }
    }
}
