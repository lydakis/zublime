use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use fs::{PathEventKind, Watcher};
use futures::StreamExt;
use gpui::{App, Context, Entity, EntityId, Render, Subscription, Task, WeakEntity, Window};
use project::{ProjectPath, Worktree, WorktreeId};
use ui::{
    ButtonCommon, Icon, IconButton, IconName, IconSize, Label, LabelSize, Tooltip, h_flex,
    prelude::*,
};

use crate::{
    DirectoryLister, SaveIntent, StatusItemView, StopWatchingFolder, ToggleWatchPause, WatchFolder,
    Workspace,
};
use anyhow::Result;

pub fn init(cx: &mut App) {
    cx.observe_new(register_actions).detach();
}

fn register_actions(
    workspace: &mut Workspace,
    _window: Option<&mut Window>,
    _: &mut Context<Workspace>,
) {
    workspace
        .register_action(|workspace, _: &WatchFolder, window, cx| {
            workspace.prompt_watch_folder(window, cx);
        })
        .register_action(|workspace, _: &ToggleWatchPause, window, cx| {
            workspace.toggle_watch_pause(window, cx);
        })
        .register_action(|workspace, _: &StopWatchingFolder, _window, cx| {
            workspace.stop_watching_folder(cx);
        });
}

#[derive(Clone)]
pub struct WatchStatus {
    state: Option<WatchStatusState>,
    workspace: WeakEntity<Workspace>,
}

#[derive(Clone)]
struct WatchStatusState {
    root_path: PathBuf,
    paused: bool,
}

impl WatchStatus {
    pub fn new(workspace: WeakEntity<Workspace>) -> Self {
        Self {
            state: None,
            workspace,
        }
    }

    pub fn set_state(&mut self, state: Option<&WatchFolderState>, cx: &mut Context<Self>) {
        self.state = state.map(|state| WatchStatusState {
            root_path: state.root_path.clone(),
            paused: state.paused,
        });
        cx.notify();
    }
}

impl StatusItemView for WatchStatus {
    fn set_active_pane_item(
        &mut self,
        _active_pane_item: Option<&dyn crate::ItemHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }
}

impl Render for WatchStatus {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(state) = &self.state else {
            return div();
        };

        let label = if state.paused {
            format!("Watching (Paused): {}", state.root_path.display())
        } else {
            format!("Watching: {}", state.root_path.display())
        };
        let workspace = self.workspace.clone();
        let pause_label = if state.paused { "Resume" } else { "Pause" };
        let pause_icon = if state.paused {
            IconName::DebugContinue
        } else {
            IconName::DebugPause
        };

        let pause_button = IconButton::new("watch-pause", pause_icon)
            .icon_size(IconSize::Small)
            .tooltip(move |_, cx| {
                Tooltip::with_meta(pause_label, None, "Toggle watching for this window", cx)
            })
            .on_click(cx.listener(move |_, _, window, cx| {
                if let Some(workspace) = workspace.upgrade() {
                    let workspace = workspace.clone();
                    window.defer(cx, move |window, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.toggle_watch_pause(window, cx);
                        });
                    });
                }
            }));

        let stop_workspace = self.workspace.clone();
        let stop_button = IconButton::new("watch-stop", IconName::Stop)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Stop watching"))
            .on_click(cx.listener(move |_, _, window, cx| {
                if let Some(workspace) = stop_workspace.upgrade() {
                    let workspace = workspace.clone();
                    window.defer(cx, move |_, cx| {
                        workspace.update(cx, |workspace, cx| {
                            workspace.stop_watching_folder(cx);
                        });
                    });
                }
            }));

        h_flex()
            .gap_1()
            .items_center()
            .child(Icon::new(IconName::Eye).size(IconSize::Small))
            .child(Label::new(label).size(LabelSize::Small))
            .child(pause_button)
            .child(stop_button)
    }
}

pub struct WatchFolderState {
    pub root_path: PathBuf,
    pub root_rel_path: util::rel_path::RelPathBuf,
    pub worktree: Entity<Worktree>,
    pub worktree_id: WorktreeId,
    pub path_style: util::paths::PathStyle,
    pub paused: bool,
    pub watcher: Arc<dyn Watcher>,
    pub watch_task: Task<()>,
    pub refresh_pending: bool,
    pub git_subscription: Subscription,
    pub watched_items: HashMap<ProjectPath, WatchedItem>,
    pub watched_item_ids: HashMap<EntityId, ProjectPath>,
    pub pending_paths: HashSet<ProjectPath>,
}

#[derive(Clone, Debug)]
pub struct WatchedItem {
    pub item_id: EntityId,
    pub close_when_clean: bool,
    pub was_dirty: bool,
}

impl Workspace {
    fn is_project_path_dirty(&self, project_path: &ProjectPath, cx: &App) -> bool {
        let git_store = self.project.read(cx).git_store();
        let git_store = git_store.read(cx);
        let Some((repo, repo_path)) =
            git_store.repository_and_path_for_project_path(project_path, cx)
        else {
            return false;
        };
        let Some(status_entry) = repo.read(cx).status_for_path(&repo_path) else {
            return false;
        };
        status_entry.status.has_changes()
    }

    pub fn watch_status_item(&self) -> &Entity<WatchStatus> {
        &self.watch_status_item
    }

    pub fn watch_folder_state(&self) -> Option<&WatchFolderState> {
        self.watch_folder.as_ref()
    }

    pub fn prompt_watch_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lister = DirectoryLister::Project(self.project.clone());
        let prompt = self.prompt_for_open_path(
            gpui::PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: None,
            },
            lister,
            window,
            cx,
        );
        let app_state = self.app_state.clone();
        let workspace_handle = cx.entity().downgrade();
        cx.spawn_in(window, async move |_, cx| -> Result<()> {
            let Ok(result) = prompt.await else {
                return Ok(());
            };
            let Some(mut paths) = result else {
                return Ok(());
            };
            let Some(path) = paths.pop() else {
                return Ok(());
            };
            let metadata = app_state.fs.metadata(&path).await?;
            if metadata.as_ref().is_none_or(|meta| !meta.is_dir) {
                return Ok(());
            }
            let canonical = app_state.fs.canonicalize(&path).await.unwrap_or(path);
            let _ = workspace_handle.update_in(cx, |workspace, window, cx| {
                workspace.start_watch_folder(canonical, window, cx);
            });
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn start_watch_folder(
        &mut self,
        root_path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_watching_folder(cx);

        self.watch_request_id = self.watch_request_id.wrapping_add(1);
        let request_id = self.watch_request_id;

        let project = self.project.clone();
        let app_state = self.app_state.clone();
        let window_handle = window.window_handle().downcast::<Workspace>().unwrap();
        let workspace_handle = self.weak_handle();

        cx.spawn_in(window, async move |_, cx| -> Result<()> {
            let project_task = cx.update(|_window, cx| {
                Workspace::project_path_for_path(project, &root_path, true, cx)
            })?;
            let (worktree, _) = project_task.await?;
            let fs = app_state.fs.clone();
            let (mut events, watcher) = fs.watch(&root_path, Duration::from_millis(150)).await;
            let path_style = worktree.read_with(cx, |worktree, _| worktree.path_style());
            let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());
            let root_rel_path = {
                let worktree_root = worktree.read_with(cx, |worktree, _| worktree.abs_path());
                let relative_root = root_path
                    .strip_prefix(worktree_root.as_ref())
                    .unwrap_or_else(|_| root_path.as_path());
                util::rel_path::RelPath::new(relative_root, path_style)?.into_owned()
            };
            let watch_task = cx.spawn({
                let window_handle = window_handle.clone();
                let root_path = root_path.clone();
                async move |cx| {
                    while let Some(batch) = events.next().await {
                        let mut candidate_paths = Vec::new();
                        for event in batch {
                            match event.kind {
                                Some(PathEventKind::Created)
                                | Some(PathEventKind::Changed)
                                | None => {
                                    if is_hidden_path(&root_path, &event.path) {
                                        continue;
                                    }
                                    if fs.is_file(&event.path).await {
                                        candidate_paths.push(event.path);
                                    }
                                }
                                Some(PathEventKind::Removed) => {}
                            }
                        }

                        if candidate_paths.is_empty() {
                            continue;
                        }

                        let _ = window_handle.update(cx, |workspace, window, cx| {
                            workspace.handle_watch_fs_paths(candidate_paths, window, cx);
                        });
                    }
                }
            });

            let _ = workspace_handle.update_in(cx, |workspace, window, cx| {
                if workspace.watch_request_id != request_id {
                    return;
                }
                let git_store = workspace.project.read(cx).git_store().clone();
                let git_subscription =
                    cx.subscribe_in(&git_store, window, |workspace, _, event, window, cx| {
                        match event {
                            project::git_store::GitStoreEvent::RepositoryUpdated(
                                _,
                                project::git_store::RepositoryEvent::StatusesChanged,
                                _,
                            )
                            | project::git_store::GitStoreEvent::RepositoryAdded
                            | project::git_store::GitStoreEvent::RepositoryRemoved(_)
                            | project::git_store::GitStoreEvent::ActiveRepositoryChanged(_)
                            | project::git_store::GitStoreEvent::ConflictsUpdated => {
                                workspace.refresh_watch_git_status(window, cx);
                            }
                            _ => {}
                        }
                    });

                workspace.watch_folder = Some(WatchFolderState {
                    root_path: root_path.clone(),
                    root_rel_path,
                    worktree: worktree.clone(),
                    worktree_id,
                    path_style,
                    paused: false,
                    watcher,
                    watch_task,
                    refresh_pending: false,
                    git_subscription,
                    watched_items: HashMap::default(),
                    watched_item_ids: HashMap::default(),
                    pending_paths: HashSet::default(),
                });
                workspace.update_watch_status_item(cx);
                workspace.refresh_watch_git_status(window, cx);
            });
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    pub fn stop_watching_folder(&mut self, cx: &mut Context<Self>) {
        if self.watch_folder.is_none() {
            return;
        }
        self.watch_folder = None;
        self.update_watch_status_item(cx);
    }

    pub fn toggle_watch_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.watch_folder.as_mut() else {
            return;
        };
        state.paused = !state.paused;
        if !state.paused {
            self.refresh_watch_git_status(window, cx);
        }
        self.update_watch_status_item(cx);
    }

    pub fn promote_watched_item(&mut self, item_id: EntityId, cx: &mut Context<Self>) {
        let Some(state) = self.watch_folder.as_mut() else {
            return;
        };
        if let Some(project_path) = state.watched_item_ids.remove(&item_id) {
            state.watched_items.remove(&project_path);
            state.pending_paths.remove(&project_path);
            cx.notify();
        }
    }

    pub fn forget_watched_item(&mut self, item_id: EntityId) {
        let Some(state) = self.watch_folder.as_mut() else {
            return;
        };
        if let Some(project_path) = state.watched_item_ids.remove(&item_id) {
            state.watched_items.remove(&project_path);
            state.pending_paths.remove(&project_path);
        }
    }

    fn update_watch_status_item(&mut self, cx: &mut Context<Self>) {
        let state = self.watch_folder.as_ref();
        self.watch_status_item
            .update(cx, |item, cx| item.set_state(state, cx));
    }

    fn handle_watch_fs_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_paths = !paths.is_empty();
        let (root_path, worktree_id, path_style, pending_paths) = {
            let Some(state) = self.watch_folder.as_ref() else {
                return;
            };
            if state.paused {
                return;
            }
            (
                state.root_path.clone(),
                state.worktree_id,
                state.path_style,
                state.pending_paths.clone(),
            )
        };

        let mut to_open = Vec::new();
        let mut new_pending = Vec::new();
        for path in paths {
            if !path.starts_with(&root_path) {
                continue;
            }
            let project_path =
                match project_path_from_abs(&path, &root_path, worktree_id, path_style) {
                    Some(path) => path,
                    None => continue,
                };
            if is_hidden_project_path(&project_path) {
                continue;
            }
            if self.item_for_project_path(&project_path, cx).is_some() {
                continue;
            }
            if pending_paths.contains(&project_path) {
                continue;
            }
            let close_when_clean = self.should_close_when_clean(&project_path, cx);
            to_open.push((project_path.clone(), close_when_clean));
            new_pending.push(project_path);
        }

        if let Some(state) = self.watch_folder.as_mut() {
            for project_path in &new_pending {
                state.pending_paths.insert(project_path.clone());
            }
        }

        for (project_path, close_when_clean) in to_open {
            self.open_watched_project_path(project_path, close_when_clean, true, window, cx);
        }

        if has_paths {
            self.schedule_watch_refresh(window, cx);
        }
    }

    fn refresh_watch_git_status(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending_paths = {
            let Some(state) = self.watch_folder.as_ref() else {
                return;
            };
            if state.paused {
                return;
            }
            state.pending_paths.clone()
        };

        let dirty_paths = self.collect_watch_dirty_paths(cx);
        let mut to_open = Vec::new();
        let mut new_pending = Vec::new();
        for dirty_path in &dirty_paths {
            if self.item_for_project_path(dirty_path, cx).is_some() {
                continue;
            }
            if pending_paths.contains(dirty_path) {
                continue;
            }
            let close_when_clean = self.should_close_when_clean(dirty_path, cx);
            to_open.push((dirty_path.clone(), close_when_clean));
            new_pending.push(dirty_path.clone());
        }

        if let Some(state) = self.watch_folder.as_mut() {
            for project_path in &new_pending {
                state.pending_paths.insert(project_path.clone());
            }
        }

        for (project_path, close_when_clean) in to_open {
            self.open_watched_project_path(project_path, close_when_clean, false, window, cx);
        }

        let mut enable_close = Vec::new();
        if let Some(state) = self.watch_folder.as_ref() {
            for (project_path, entry) in state.watched_items.iter() {
                if !entry.close_when_clean && self.should_close_when_clean(project_path, cx) {
                    enable_close.push(project_path.clone());
                }
            }
        }
        if let Some(state) = self.watch_folder.as_mut() {
            for project_path in enable_close {
                if let Some(entry) = state.watched_items.get_mut(&project_path) {
                    entry.close_when_clean = true;
                    entry.was_dirty = true;
                }
            }
        }

        let mut to_close = Vec::new();
        if let Some(state) = self.watch_folder.as_mut() {
            for (project_path, entry) in state.watched_items.iter_mut() {
                if dirty_paths.contains(project_path) {
                    entry.was_dirty = true;
                    continue;
                }
                if entry.close_when_clean && entry.was_dirty {
                    to_close.push(entry.item_id);
                }
            }
        }
        for item_id in to_close {
            self.close_watched_item(item_id, window, cx);
        }
    }

    fn schedule_watch_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.watch_folder.as_mut() else {
            return;
        };
        if state.refresh_pending {
            return;
        }
        state.refresh_pending = true;

        let window_handle = window.window_handle().downcast::<Workspace>().unwrap();
        let workspace_handle = self.weak_handle();
        cx.spawn_in(window, async move |_, cx| -> Result<()> {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let _ = window_handle.update(cx, |workspace, window, cx| {
                let _ = workspace_handle;
                if let Some(state) = workspace.watch_folder.as_mut() {
                    state.refresh_pending = false;
                }
                workspace.refresh_watch_git_status(window, cx);
            });
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn collect_watch_dirty_paths(&self, cx: &App) -> HashSet<ProjectPath> {
        let Some(state) = self.watch_folder.as_ref() else {
            return HashSet::default();
        };
        let root_rel_path = state.root_rel_path.as_rel_path();
        let git_store = self.project.read(cx).git_store();
        let git_store = git_store.read(cx);
        let mut dirty_paths = HashSet::default();

        for repository in git_store.repositories().values() {
            let repo = repository.read(cx);
            for status_entry in repo.cached_status() {
                if !status_entry.status.has_changes() {
                    continue;
                }
                let Some(project_path) = repository
                    .read(cx)
                    .repo_path_to_project_path(&status_entry.repo_path, cx)
                else {
                    continue;
                };
                if project_path.worktree_id != state.worktree_id {
                    continue;
                }
                if !project_path.path.starts_with(root_rel_path) {
                    continue;
                }
                if is_hidden_project_path(&project_path) {
                    continue;
                }
                dirty_paths.insert(project_path);
            }
        }
        dirty_paths
    }

    fn should_close_when_clean(&self, project_path: &ProjectPath, cx: &App) -> bool {
        let git_store = self.project.read(cx).git_store();
        git_store
            .read(cx)
            .repository_and_path_for_project_path(project_path, cx)
            .is_some()
    }

    fn open_watched_project_path(
        &mut self,
        project_path: ProjectPath,
        close_when_clean: bool,
        opened_from_fs: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task =
            self.open_path_preview(project_path.clone(), None, false, false, false, window, cx);
        let workspace_handle = self.weak_handle();
        cx.spawn_in(window, async move |_, cx| -> Result<()> {
            let item = match task.await {
                Ok(item) => item,
                Err(error) => {
                    workspace_handle
                        .update(cx, |workspace, _| {
                            if let Some(state) = workspace.watch_folder.as_mut() {
                                state.pending_paths.remove(&project_path);
                            }
                        })
                        .ok();
                    return Err(error);
                }
            };
            let item_id = item.item_id();
            workspace_handle.update(cx, |workspace, cx| {
                let was_dirty = if close_when_clean {
                    opened_from_fs || workspace.is_project_path_dirty(&project_path, cx)
                } else {
                    false
                };
                let Some(state) = workspace.watch_folder.as_mut() else {
                    return;
                };
                state.pending_paths.remove(&project_path);
                state.watched_item_ids.insert(item_id, project_path.clone());
                state.watched_items.insert(
                    project_path,
                    WatchedItem {
                        item_id,
                        close_when_clean,
                        was_dirty,
                    },
                );
                cx.notify();
            })?;
            Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn close_watched_item(
        &mut self,
        item_id: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(weak_pane) = self.panes_by_item.get(&item_id) else {
            return;
        };
        let Some(pane) = weak_pane.upgrade() else {
            return;
        };
        self.forget_watched_item(item_id);
        pane.update(cx, |pane, cx| {
            pane.close_item_by_id(item_id, SaveIntent::Close, window, cx)
                .detach_and_log_err(cx);
        });
    }

    fn item_for_project_path(&self, project_path: &ProjectPath, cx: &App) -> Option<EntityId> {
        self.items(cx)
            .find(|item| item.project_path(cx).as_ref() == Some(project_path))
            .map(|item| item.item_id())
    }
}

fn project_path_from_abs(
    abs_path: &Path,
    root_path: &Path,
    worktree_id: WorktreeId,
    path_style: util::paths::PathStyle,
) -> Option<ProjectPath> {
    let rel_path = abs_path.strip_prefix(root_path).ok()?;
    let rel_path = util::rel_path::RelPath::new(rel_path, path_style)
        .ok()?
        .into_owned();
    Some(ProjectPath {
        worktree_id,
        path: rel_path.into(),
    })
}

fn is_hidden_path(root_path: &Path, abs_path: &Path) -> bool {
    let rel = abs_path.strip_prefix(root_path).unwrap_or(abs_path);
    rel.components().any(|component| match component {
        std::path::Component::Normal(name) => name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

fn is_hidden_project_path(project_path: &ProjectPath) -> bool {
    project_path
        .path
        .components()
        .any(|component| component.starts_with('.'))
}
