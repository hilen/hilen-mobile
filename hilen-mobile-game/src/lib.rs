#![allow(incomplete_features)]
#![feature(specialization)]
#![feature(arbitrary_self_types)]

use hilen::{
    App,
    refs::{Own, Weak},
    ui::{Button, Label, Setup, UIManager, View, ViewData, view},
};

#[view]
struct MobileView {
    #[init]
    label:  Label,
    button: Button,
}

impl Setup for MobileView {
    fn setup(self: Weak<Self>) {
        self.place().all_ver();

        self.label.set_text("Hello");
        self.button.set_text("Press").on_tap(|| {
            dbg!(UIManager::cloud_storage_dir());
        });
    }
}

#[derive(Default)]
struct MobileApp {}

impl App for MobileApp {
    fn make_root_view(&self) -> Own<dyn View> {
        MobileView::new()
    }
}

hilen::register_app!(MobileApp);
