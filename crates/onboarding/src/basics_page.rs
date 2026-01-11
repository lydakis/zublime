use fs::Fs;
use gpui::{App, IntoElement};
use settings::{BaseKeymap, Settings, update_settings_file};
use ui::{
    SwitchField, ToggleButtonGroup, ToggleButtonGroupSize, ToggleButtonWithIcon, ToggleState,
    prelude::*,
};
use vim_mode_setting::VimModeSetting;

fn render_base_keymap_section(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let base_keymap = match BaseKeymap::get_global(cx) {
        BaseKeymap::VSCode => Some(0),
        BaseKeymap::JetBrains => Some(1),
        BaseKeymap::SublimeText => Some(2),
        BaseKeymap::Atom => Some(3),
        BaseKeymap::Emacs => Some(4),
        BaseKeymap::Cursor => Some(5),
        BaseKeymap::TextMate | BaseKeymap::None => None,
    };

    v_flex()
        .gap_2()
        .child(Label::new("Shortcuts"))
        .child(
            ToggleButtonGroup::two_rows(
                "base_keymap_selection",
                [
                    ToggleButtonWithIcon::new("VS Code", IconName::EditorVsCode, |_, _, cx| {
                        write_keymap_base(BaseKeymap::VSCode, cx);
                    }),
                    ToggleButtonWithIcon::new("JetBrains", IconName::EditorJetBrains, |_, _, cx| {
                        write_keymap_base(BaseKeymap::JetBrains, cx);
                    }),
                    ToggleButtonWithIcon::new("Sublime Text", IconName::EditorSublime, |_, _, cx| {
                        write_keymap_base(BaseKeymap::SublimeText, cx);
                    }),
                ],
                [
                    ToggleButtonWithIcon::new("Atom", IconName::EditorAtom, |_, _, cx| {
                        write_keymap_base(BaseKeymap::Atom, cx);
                    }),
                    ToggleButtonWithIcon::new("Emacs", IconName::EditorEmacs, |_, _, cx| {
                        write_keymap_base(BaseKeymap::Emacs, cx);
                    }),
                    ToggleButtonWithIcon::new("Cursor", IconName::EditorCursor, |_, _, cx| {
                        write_keymap_base(BaseKeymap::Cursor, cx);
                    }),
                ],
            )
            .when_some(base_keymap, |this, base_keymap| {
                this.selected_index(base_keymap)
            })
            .full_width()
            .tab_index(tab_index)
            .size(ToggleButtonGroupSize::Medium)
            .style(ui::ToggleButtonGroupStyle::Outlined),
        )
}

fn render_vim_mode_switch(tab_index: &mut isize, cx: &mut App) -> impl IntoElement {
    let toggle_state = if VimModeSetting::get_global(cx).0 {
        ToggleState::Selected
    } else {
        ToggleState::Unselected
    };

    SwitchField::new(
        "onboarding-vim-mode",
        Some("Vim Mode"),
        Some("Enable Vim keybindings".into()),
        toggle_state,
        {
            let fs = <dyn Fs>::global(cx);
            move |&selection, _, cx| {
                let vim_mode = match selection {
                    ToggleState::Selected => true,
                    ToggleState::Unselected => false,
                    ToggleState::Indeterminate => {
                        return;
                    }
                };
                update_settings_file(fs.clone(), cx, move |setting, _| {
                    setting.vim_mode = Some(vim_mode);
                });
            }
        },
    )
    .tab_index({
        *tab_index += 1;
        *tab_index - 1
    })
}

pub(crate) fn render_basics_page(cx: &mut App) -> impl IntoElement {
    let mut tab_index = 0;
    v_flex()
        .id("basics-page")
        .gap_6()
        .child(
            v_flex()
                .gap_0p5()
                .child(Headline::new("Quick Setup").size(HeadlineSize::Small))
                .child(
                    Label::new("Choose shortcuts and editing mode.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(render_base_keymap_section(&mut tab_index, cx))
        .child(render_vim_mode_switch(&mut tab_index, cx))
}

fn write_keymap_base(keymap_base: BaseKeymap, cx: &App) {
    let fs = <dyn Fs>::global(cx);

    update_settings_file(fs, cx, move |setting, _| {
        setting.base_keymap = Some(keymap_base.into());
    });
}
