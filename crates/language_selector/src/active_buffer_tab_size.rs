use editor::Editor;
use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window, div,
};
use text::Point;
use ui::{Button, ButtonCommon, Clickable, LabelSize, Tooltip};
use workspace::{StatusItemView, Workspace, item::ItemHandle};

pub struct ActiveBufferTabSize {
    tab_size: Option<u32>,
    _observe_active_editor: Option<Subscription>,
}

impl ActiveBufferTabSize {
    pub fn new(_workspace: &Workspace) -> Self {
        Self {
            tab_size: None,
            _observe_active_editor: None,
        }
    }

    fn update_tab_size(&mut self, editor: Entity<Editor>, _: &mut Window, cx: &mut Context<Self>) {
        self.tab_size = None;

        self.tab_size = editor.update(cx, |editor, cx| {
            if editor.active_excerpt(cx).is_some() {
                let snapshot = editor.display_snapshot(cx);
                let selection = editor.selections.newest::<Point>(&snapshot);
                let head = selection.head();
                Some(
                    editor
                        .buffer()
                        .read(cx)
                        .language_settings_at(head, cx)
                        .tab_size
                        .get(),
                )
            } else {
                None
            }
        });

        cx.notify();
    }
}

impl Render for ActiveBufferTabSize {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(tab_size) = self.tab_size else {
            return div().hidden();
        };

        div().child(
            Button::new("tab-size", format!("Tab: {tab_size}"))
                .label_size(LabelSize::Small)
                .on_click(|_, _, _cx| {
                    // no-op
                })
                .tooltip(Tooltip::text("Tab Size")),
        )
    }
}

impl StatusItemView for ActiveBufferTabSize {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = active_pane_item.and_then(|item| item.downcast::<Editor>()) {
            self._observe_active_editor =
                Some(cx.observe_in(&editor, window, Self::update_tab_size));
            self.update_tab_size(editor, window, cx);
        } else {
            self.tab_size = None;
            self._observe_active_editor = None;
        }

        cx.notify();
    }
}
