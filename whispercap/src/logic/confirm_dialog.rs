use crate::global_logic;
use crate::slint_generatedAppWindow::{AppWindow, Logic, Util};
use slint::ComponentHandle;

pub fn init(ui: &AppWindow) {
    let ui_handle = ui.as_weak();
    ui.global::<Util>()
        .on_handle_confirm_dialog(move |handle_type, user_data| {
            let ui = ui_handle.unwrap();

            match handle_type.as_str() {
                "remove-caches" => {
                    ui.global::<Logic>().invoke_remove_caches();
                }
                "uninstall" => {
                    ui.global::<Logic>().invoke_uninstall();
                }
                "close-window" => {
                    ui.global::<Util>().invoke_close_window();
                }
                "remove-all-corrected-subtitles" => {
                    global_logic!(ui).invoke_remove_all_corrected_subtitles();
                }
                "remove-all-translated-subtitles" => {
                    global_logic!(ui).invoke_remove_all_translated_subtitles();
                }
                "remove-all-subtitles" => {
                    global_logic!(ui).invoke_remove_all_subtitles();
                }
                "remove-subtitle" => {
                    let index = user_data.parse::<i32>().unwrap_or_default();
                    global_logic!(ui).invoke_remove_subtitle(index);
                }
                "remove-model" => {
                    let index = user_data.parse::<i32>().unwrap_or_default();
                    global_logic!(ui).invoke_remove_model(index);
                }
                _ => (),
            }
        });
}
