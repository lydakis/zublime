use editor::Editor;
use gpui::{Context, Entity, EntityId, Render, Styled, Subscription, WeakEntity, Window, div};
use language::{Buffer, Capability};
use ui::{IconButton, IconButtonShape, IconName, IconSize, SharedString, Tooltip};
use workspace::{Pane, StatusItemView, Workspace, item::ItemHandle};

use crate::file_diff_view::FileDiffView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiffBase {
    Head,
    Staged,
}

pub struct ActiveBufferGitDiff {
    workspace: WeakEntity<Workspace>,
    show_button: bool,
    _observe_active_editor: Option<Subscription>,
}

impl ActiveBufferGitDiff {
    pub fn new(workspace: &Workspace) -> Self {
        Self {
            workspace: workspace.weak_handle(),
            show_button: false,
            _observe_active_editor: None,
        }
    }

    fn update_for_editor(&mut self, editor: Entity<Editor>, _: &mut Window, cx: &mut Context<Self>) {
        self.show_button = false;

        let Some(workspace) = self.workspace.upgrade() else {
            cx.notify();
            return;
        };

        let Some((_, buffer, _)) = editor.active_excerpt(cx) else {
            cx.notify();
            return;
        };

        let buffer_id = buffer.read(cx).remote_id();
        let in_repo = workspace
            .read(cx)
            .project()
            .git_store()
            .read(cx)
            .repository_and_path_for_buffer_id(buffer_id, cx)
            .is_some();

        self.show_button = in_repo;

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
        if !self.show_button {
            return div().hidden();
        }

        IconButton::new("status_git_diff", IconName::Diff)
            .icon_size(IconSize::Small)
            .shape(IconButtonShape::Square)
            .tooltip(Tooltip::text("Open Git Diff (Alt for staged)"))
            .on_click(cx.listener(Self::open_diff))
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
        pane.add_item(Box::new(editor.clone()), true, true, Some(destination_index), window, cx);
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
    let Some((_, buffer, _)) = editor.active_excerpt(cx) else {
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
        .spawn(cx, async move |cx| {
            let diff = diff_task.await?;
            let base_text = diff
                .read_with(cx, |diff, _| diff.base_text_string())
                .unwrap_or_default();
            let language = buffer.read_with(cx, |buffer, _| buffer.language().cloned());
            let old_buffer = cx.new(|cx| {
                let mut buffer = Buffer::local(base_text, cx);
                if let Some(language) = language {
                    buffer = buffer.with_language(language, cx);
                }
                buffer.set_capability(Capability::ReadOnly, cx);
                buffer
            });

            let Some(workspace) = workspace.upgrade() else {
                return Ok(());
            };
            let Some(pane) = pane.upgrade() else {
                return Ok(());
            };

            let diff_view_task = workspace.update_in(cx, |workspace, window, cx| {
                FileDiffView::open_buffers_in_pane(
                    old_buffer,
                    buffer,
                    Some(label),
                    None,
                    pane.clone(),
                    Some(destination_index),
                    workspace,
                    window,
                    cx,
                )
            })?;
            let _ = diff_view_task.await?;
            workspace.update_in(cx, |_, window, cx| {
                pane.update(cx, |pane, cx| {
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
    let pane = workspace.active_pane();
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
            self._observe_active_editor =
                Some(cx.observe_in(&editor, window, Self::update_for_editor));
            self.update_for_editor(editor, window, cx);
        } else {
            self.show_button = false;
            self._observe_active_editor = None;
        }

        cx.notify();
    }
}
