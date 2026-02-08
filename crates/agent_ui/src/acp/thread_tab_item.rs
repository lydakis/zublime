use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, Render, SharedString,
    Window,
};
use workspace::item::Item;

use crate::acp::AcpServerView;

pub struct AcpThreadTabItem(pub Entity<AcpServerView>);

impl Item for AcpThreadTabItem {
    type Event = ();

    fn include_in_nav_history() -> bool {
        false
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        self.0.read(cx).title(cx)
    }
}

impl EventEmitter<()> for AcpThreadTabItem {}

impl Focusable for AcpThreadTabItem {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus_handle(cx)
    }
}

impl Render for AcpThreadTabItem {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}
