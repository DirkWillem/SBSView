use std::collections::LinkedList;
use std::rc::Rc;
use std::sync::Arc;
use eframe::egui;

use eframe::egui::Ui;
use egui_plot::Plot;
use tokio::sync::Mutex;

use sbs_core::sbs::Client;
use sbs_uart::sbs_uart::SbsUart;

use crate::view::{AsyncProcess, ChildView, State, TopLevelView, View};
use crate::views::connect_view::{ConnectView, Port};
use crate::views::signals_view::{SignalsView, SignalsViewAction};

pub enum MainViewAction {
    Connect(Port),
    ConnectSuccess(Box<dyn Client + Send>),
    ConnectFailed(String),
}

pub enum ConnectState {
    Disconnected,
    Connecting(AsyncProcess<Result<Box<SbsUart>, String>>),
    Connected,
}

pub struct MainViewState {
    connect_state: ConnectState,
    client: Option<Arc<Mutex<Box<dyn Client + Send>>>>,
}

impl State<MainViewAction> for MainViewState {
    fn apply(&mut self, action: MainViewAction) {
        match action {
            MainViewAction::Connect(port) => self.connect(port),
            MainViewAction::ConnectSuccess(client) => {
                self.client = Some(Arc::new(Mutex::new(client)));
                self.connect_state = ConnectState::Connected;
            }
            MainViewAction::ConnectFailed(err) => {
                println!("Connect failed: {err}");
                self.connect_state = ConnectState::Disconnected;
            }
        }
    }
}

impl MainViewState {
    pub fn new() -> MainViewState {
        MainViewState {
            connect_state: ConnectState::Disconnected,
            client: None,
        }
    }

    fn connect(&mut self, port: Port) {
        match port {
            Port::SerialPort(port_name) => {
                self.connect_state = ConnectState::Connecting(AsyncProcess::<Result<Box<SbsUart>, String>>::new({
                    async move {
                        let mut result = Box::new(SbsUart::new());
                        let connect_result = result.connect(&port_name, 115_200).await;

                        match connect_result {
                            Ok(_) => Ok(result),
                            Err(e) => Err(e.to_string())
                        }
                    }
                }
                ));
            }
        }
    }

    fn check_connecting_state(&mut self) -> Option<MainViewAction> {
        match &mut self.connect_state {
            ConnectState::Disconnected => None,
            ConnectState::Connecting(ref mut proc) => {
                if proc.is_done() {
                    let client = proc.get();

                    match client {
                        Ok(client) => Some(MainViewAction::ConnectSuccess(client)),
                        Err(e) => Some(MainViewAction::ConnectFailed(e))
                    }
                } else {
                    None
                }
            }
            ConnectState::Connected => None
        }
    }
}


pub struct MainView {
    state: MainViewState,

    connect_view: ConnectView,
    signals_view: Option<SignalsView>,
}

impl MainView {
    pub fn new() -> MainView {
        MainView {
            state: MainViewState::new(),
            connect_view: ConnectView::new(),
            signals_view: None,
        }
    }
}

impl TopLevelView<MainViewState, MainViewAction> for MainView {
    fn state(&mut self) -> &mut MainViewState {
        &mut self.state
    }

    fn view(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> LinkedList<MainViewAction> {
        let mut result = LinkedList::<MainViewAction>::default();

        if let Some(action) = self.state.check_connecting_state() {
            result.push_back(action);
        }

        match &self.state.connect_state {
            ConnectState::Disconnected => {
                result.append(&mut self.view_disconnected(ctx, frame));
            }
            ConnectState::Connecting(_) => {
                // ui.spinner();
                self.view_connecting(ctx, frame);
            }
            ConnectState::Connected => {
                self.view_connected(ctx, frame);
            }
            _ => {}
        };


        result
    }
}

impl MainView {
    fn view_disconnected(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> LinkedList<MainViewAction> {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.connect_view.render(ui).into()
        }).inner
    }

    fn view_connecting(&self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| ui.spinner())
        });
    }

    fn view_connected(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> LinkedList<MainViewAction> {
        if self.signals_view.is_none() {
            let mut signals_view = SignalsView::new(self.state.client.as_ref().unwrap().clone());
            signals_view.state().apply(SignalsViewAction::FetchSignals);
            self.signals_view = Some(signals_view);
        }

        let mut result = LinkedList::<MainViewAction>::default();

        let mut signals_view_actions = egui::SidePanel::left("signals")
            .exact_width(240.0)
            .show(ctx, |ui| {
                self.signals_view.as_mut().unwrap().render(ui)
            }).inner;

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                let plot = Plot::new(1)
                    .show_axes(true)
                    .show_grid(true);
                plot.show(ui, |plot_ui| {

                });
            });

        result.append(&mut signals_view_actions);
        result
    }
}
