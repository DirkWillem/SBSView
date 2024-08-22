use crate::view::{State, View};
use crate::views::main_view::MainViewAction;
use eframe::egui;
use eframe::egui::{Align, InnerResponse, Ui};
use regex::Regex;
use std::collections::LinkedList;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug)]
pub enum ConnectViewAction {
    Rescan,
    Connect(Port),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Port {
    SerialPort(String)
}

impl Display for Port {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Port::SerialPort(port_name) => write!(f, "Serial - {port_name}"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ConnectViewState {
    available_ports: Vec<Port>,
    selected_port: Option<Port>,
}

impl State<ConnectViewAction> for ConnectViewState {
    fn apply(&mut self, action: ConnectViewAction) {
        match action {
            ConnectViewAction::Rescan => self.rescan(),
            ConnectViewAction::Connect(_) => {}
        }
    }
}

impl ConnectViewState {
    fn rescan(&mut self) {
        if let Ok(ports) = serialport::available_ports() {
            let (mut port_names, mut unlikely_port_names): (Vec<_>, Vec<_>) = ports
                .iter()
                .map(|p| p.port_name.clone())
                .partition(|p| Self::is_likely_port_name(p));

            port_names.append(&mut unlikely_port_names);

            self.available_ports = port_names
                .into_iter()
                .map(|p| Port::SerialPort(p))
                .collect::<Vec<_>>();

            if let Some(prev_selected) = self.selected_port.take() {
                if self.available_ports.contains(&prev_selected) {
                    self.selected_port = Some(prev_selected);
                } else {
                    self.selected_port = self.available_ports.first().cloned();
                }
            } else {
                self.selected_port = self.available_ports.first().cloned();
            }
        }
    }

    fn is_likely_port_name(port_name: &str) -> bool {
        let macos_usb = Regex::new("^/dev/tty.usb[a-zA-Z0-9]+$").expect("invalid regex");

        macos_usb.is_match(port_name)
    }
}

#[derive(Clone, Debug)]
pub struct ConnectView {
    state: ConnectViewState,
}

impl ConnectView {
    pub fn new() -> ConnectView {
        let mut state = ConnectViewState::default();
        state.rescan();

        ConnectView {
            state
        }
    }
}

impl View<ConnectViewState, ConnectViewAction, MainViewAction> for ConnectView {
    fn state(&mut self) -> &mut ConnectViewState {
        &mut self.state
    }

    fn view(&mut self, ui: &mut Ui) -> InnerResponse<LinkedList<ConnectViewAction>> {
        let mut result = LinkedList::<ConnectViewAction>::default();
        ui.group(|ui| {
            ui.heading("Connect");

            ui.with_layout(egui::Layout::left_to_right(Align::LEFT), |ui| {
                egui::ComboBox::from_id_source("serial_port_combo")
                    .selected_text(self.state.selected_port
                        .as_ref()
                        .map(|p| p.to_string())
                        .unwrap_or("No port selected".to_string()))
                    .show_ui(ui, |ui| {
                        for port in &self.state.available_ports {
                            ui.selectable_value(&mut self.state.selected_port, Some(port.clone()), format!("🔌 {}", port.to_string()));
                        }
                    });

                if ui.add(egui::Button::new("Rescan")).clicked() {
                    result.push_back(ConnectViewAction::Rescan);
                }
            });

            if ui.add_enabled(
                self.state.selected_port.is_some(),
                egui::Button::new("Connect"),
            ).clicked() {
                result.push_back(ConnectViewAction::Connect(self.state.selected_port.clone().unwrap()));
            }

            result
        })
    }

    fn action_to_parent_action(&self, action: &ConnectViewAction) -> Option<MainViewAction> {
        match action {
            ConnectViewAction::Connect(port) =>
                Some(MainViewAction::Connect(port.clone())),
            _ => None
        }
    }
}
