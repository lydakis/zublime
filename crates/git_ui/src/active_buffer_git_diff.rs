use anyhow::Result;
use editor::Editor;
use gpui::{
    AppContext, Context, Entity, EntityId, Render, Styled, Subscription, WeakEntity, Window, div,
};
use language::Capability;
use project::git_store::GitStoreEvent;
use ui::{IconButton, IconButtonShape, IconName, IconSize, SharedString, Tooltip, prelude::*};
use workspace::{Pane, ProjectItem, StatusItemView, Workspace, item::ItemHandle};

use crate::file_diff_view::FileDiffView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffBase {
    Head,
    Staged,
}

pub struct ActiveBufferGitDiff {
    workspace: WeakEntity<Workspace>,
    project: WeakEntity<project::Project>,
    active_editor: Option<WeakEntity<Editor>>,
    _observe_active_editor: Option<Subscription>,
    _observe_git_store: Option<Subscription>,
}

impl ActiveBufferGitDiff {
    pub fn new(workspace: &Workspace) -> Self {
        Self {
            workspace: workspace.weak_handle(),
            project: workspace.project().clone().downgrade(),
            active_editor: None,
            _observe_active_editor: None,
            _observe_git_store: None,
        }
    }

    fn update_for_editor(
        &mut self,
        editor: Entity<Editor>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_editor = Some(editor.downgrade());
        cx.notify();
    }

    fn open_diff(&mut self, event: &gpui::ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let base = if event.modifiers().alt {
            DiffBase::Staged
        } else {
            DiffBase::Head
        };

        workspace.update(cx, |workspace, cx| {
            toggle_active_buffer_git_diff(workspace, base, window, cx);
        });
    }
}

impl Render for ActiveBufferGitDiff {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl ui::IntoElement {
        let Some(project) = self.project.upgrade() else {
            return div().hidden();
        };
        let Some(editor) = self
            .active_editor
            .as_ref()
            .and_then(|editor| editor.upgrade())
        else {
            return div().hidden();
        };
        let Some((_, buffer, _)) = editor.read(cx).active_excerpt(cx) else {
            return div().hidden();
        };
        let buffer_id = buffer.read(cx).remote_id();
        let in_repo = project
            .read(cx)
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer_id, cx)
            .is_some();

        if !in_repo {
            return div().hidden();
        }

        div().child(
            IconButton::new("status_git_diff", IconName::Diff)
                .icon_size(IconSize::Small)
                .shape(IconButtonShape::Square)
                .tooltip(Tooltip::text("Open Git Diff (Alt for staged)"))
                .on_click(cx.listener(Self::open_diff)),
        )
    }
}

fn diff_label(base: DiffBase) -> SharedString {
    match base {
        DiffBase::Head => "HEAD".into(),
        DiffBase::Staged => "STAGED".into(),
    }
}

fn replace_diff_with_editor(
    diff_view: Entity<FileDiffView>,
    diff_item_id: EntityId,
    destination_index: usize,
    pane: Entity<Pane>,
    project: Entity<project::Project>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let buffer = diff_view.read(cx).new_buffer();
    let editor = pane.update(cx, |pane, cx| {
        cx.new(|cx| Editor::for_project_item(project.clone(), Some(pane), buffer, window, cx))
    });

    pane.update(cx, |pane, cx| {
        pane.remove_item(diff_item_id, false, false, window, cx);
        pane.add_item(
            Box::new(editor.clone()),
            true,
            true,
            Some(destination_index),
            window,
            cx,
        );
    });
}

fn open_diff_for_editor(
    editor: Entity<Editor>,
    base: DiffBase,
    item_id: EntityId,
    destination_index: usize,
    pane: Entity<Pane>,
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some((_, buffer, _)) = editor.read(cx).active_excerpt(cx) else {
        return;
    };
    let buffer_id = buffer.read(cx).remote_id();
    let in_repo = workspace
        .project()
        .read(cx)
        .git_store()
        .read(cx)
        .repository_and_path_for_buffer_id(buffer_id, cx)
        .is_some();
    if !in_repo {
        return;
    }

    let project = workspace.project().clone();
    let diff_task = project.update(cx, |project, cx| match base {
        DiffBase::Head => project.open_uncommitted_diff(buffer.clone(), cx),
        DiffBase::Staged => project.open_unstaged_diff(buffer.clone(), cx),
    });
    let label = diff_label(base);

    let workspace = cx.entity().downgrade();
    let pane = pane.downgrade();
    window
        .spawn(cx, async move |cx| -> Result<()> {
            let diff = diff_task.await?;
            let language = buffer.read_with(cx, |buffer, _| buffer.language().cloned());
            let old_buffer = diff.read_with(cx, |diff, _| diff.base_text_buffer());
            old_buffer.update(cx, |buffer, cx| {
                if let Some(language) = language {
                    buffer.set_language(Some(language), cx);
                }
                buffer.set_capability(Capability::ReadOnly, cx);
            });

            let Some(workspace) = workspace.upgrade() else {
                return Ok(());
            };
            let Some(pane) = pane.upgrade() else {
                return Ok(());
            };

            workspace.update_in(cx, |_workspace, window, cx| {
                let workspace_handle = cx.entity();
                let diff_view = cx.new(|cx| {
                    FileDiffView::new(
                        old_buffer.clone(),
                        buffer.clone(),
                        diff.clone(),
                        project.clone(),
                        workspace_handle,
                        Some(label),
                        None,
                        window,
                        cx,
                    )
                });
                pane.update(cx, |pane, cx| {
                    pane.add_item(
                        Box::new(diff_view.clone()),
                        true,
                        true,
                        Some(destination_index),
                        window,
                        cx,
                    );
                    pane.remove_item(item_id, false, false, window, cx);
                });
            })?;

            Ok(())
        })
        .detach();
}

pub fn toggle_active_buffer_git_diff(
    workspace: &mut Workspace,
    base: DiffBase,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let pane = workspace.active_pane().clone();
    let (active_item, active_index) = {
        let pane = pane.read(cx);
        (pane.active_item(), pane.active_item_index())
    };
    let Some(active_item) = active_item else {
        return;
    };
    let active_item_id = active_item.item_id();

    if let Some(diff_view) = active_item.downcast::<FileDiffView>() {
        let project = workspace.project().clone();
        replace_diff_with_editor(
            diff_view,
            active_item_id,
            active_index,
            pane,
            project,
            window,
            cx,
        );
        return;
    }

    let Some(editor) = active_item.downcast::<Editor>() else {
        return;
    };
    open_diff_for_editor(
        editor,
        base,
        active_item_id,
        active_index,
        pane,
        workspace,
        window,
        cx,
    );
}

impl StatusItemView for ActiveBufferGitDiff {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = active_pane_item.and_then(|item| item.downcast::<Editor>()) {
            self.active_editor = Some(editor.downgrade());
            self._observe_active_editor =
                Some(cx.observe_in(&editor, window, Self::update_for_editor));
            self.update_for_editor(editor, window, cx);
        } else {
            self.active_editor = None;
            self._observe_active_editor = None;
        }

        if self._observe_git_store.is_none() {
            let Some(project) = self.project.upgrade() else {
                cx.notify();
                return;
            };
            let git_store = project.read(cx).git_store().clone();
            self._observe_git_store = Some(cx.subscribe(&git_store, |_this, _, event, cx| {
                if matches!(
                    event,
                    GitStoreEvent::RepositoryAdded
                        | GitStoreEvent::RepositoryRemoved(_)
                        | GitStoreEvent::RepositoryUpdated(_, _, _)
                        | GitStoreEvent::ActiveRepositoryChanged(_)
                ) {
                    cx.notify();
                }
            }));
        }

        cx.notify();
    }
}
