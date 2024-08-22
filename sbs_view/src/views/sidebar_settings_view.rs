use std::collections::LinkedList;
use eframe::egui::{ComboBox, InnerResponse, Ui};
use crate::view::{State, View};

pub enum SidebarSettingsAction {}

pub struct SidebarSettingsState {}

impl State<SidebarSettingsAction> for SidebarSettingsState {
    fn apply(&mut self, action: SidebarSettingsAction) {}
}

impl SidebarSettingsState {
    pub fn new() -> SidebarSettingsState {
        SidebarSettingsState {}
    }
}


pub struct SidebarSettingsView {
    state: SidebarSettingsState,
}

impl View<SidebarSettingsState, SidebarSettingsAction, ()> for SidebarSettingsView {
    fn state(&mut self) -> &mut SidebarSettingsState {
        &mut self.state
    }

    fn view(&mut self, ui: &mut Ui) -> InnerResponse<LinkedList<SidebarSettingsAction>> {
        ComboBox::from_id_source("Layout").selected_text("2x2").show_ui(ui, |ui| {
            ui.selectable_label(false, "Single Plot");
            ui.selectable_label(false, "2 Split Horizontal");
            ui.selectable_label(false, "2 Split Vertical");
            ui.selectable_label(true, "2x2 Grid");
        });

        InnerResponse::new(LinkedList::<SidebarSettingsAction>::new(), ui.label("Hoi"))
    }
}

impl SidebarSettingsView {
    pub fn new() -> SidebarSettingsView {
        SidebarSettingsView {
            state: SidebarSettingsState::new(),
        }
    }
}
