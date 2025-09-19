use crate::slint_generatedAppWindow::AppWindow;

#[cfg(any(feature = "desktop", feature = "mobile"))]
mod about;

#[cfg(any(feature = "desktop", feature = "mobile"))]
mod util;

#[cfg(any(feature = "desktop", feature = "mobile"))]
mod setting;

#[cfg(any(feature = "desktop", feature = "mobile"))]
mod clipboard;

mod confirm_dialog;
mod popup_action;
mod toast;
mod tr;

mod model;
mod transcribe;

#[macro_export]
macro_rules! global_store {
    ($ui:expr) => {
        $ui.global::<crate::slint_generatedAppWindow::Store>()
    };
}

#[macro_export]
macro_rules! global_logic {
    ($ui:expr) => {
        $ui.global::<crate::slint_generatedAppWindow::Logic>()
    };
}

#[macro_export]
macro_rules! global_util {
    ($ui:expr) => {
        $ui.global::<crate::slint_generatedAppWindow::Util>()
    };
}

pub fn init(ui: &AppWindow) {
    #[cfg(any(feature = "desktop", feature = "mobile"))]
    {
        util::init(ui);
        clipboard::init(ui);
        about::init(ui);
        setting::init(ui);
    }

    toast::init(ui);
    confirm_dialog::init(ui);
    popup_action::init(ui);

    {
        transcribe::init(ui);
        model::init(ui);
    }
}
