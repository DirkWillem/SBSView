use eframe::egui;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, LinkedList};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use eframe::egui::{Response, Ui};
use pollster::FutureExt;
use tokio::sync::Mutex;

use crate::signals::window_buffer::WindowBuffer;
use crate::view::{AsyncProcess, ChildView, State, TopLevelView, View};
use crate::views::connect_view::{ConnectView, Port};
use crate::views::plot_view::{PlotView, PlotViewParentAction};
use crate::views::sidebar_settings_view::SidebarSettingsView;
use crate::views::signals_view::{SignalsView, SignalsViewAction};
use sbs_core::sbs::{Client, SignalId};
use sbs_uart::sbs_uart::SbsUart;

pub enum MainViewAction {
    SetActivePlot(u32),

    Connect(Port),
    ConnectSuccess(Box<dyn Client + Send>),
    ConnectFailed(String),

    AddSignalToCurrentPlot(SignalId),
    RemoveSignalFromCurrentPlot(SignalId),
}

pub enum ConnectState {
    Disconnected,
    Connecting(AsyncProcess<Result<Box<SbsUart>, String>>),
    Connected,
}

pub struct PlotState {
    #[allow(dead_code)]
    enabled_signals: HashSet<SignalId>,
    window_buffer: Rc<RefCell<WindowBuffer>>,
}

impl PlotState {
    pub fn new(window_buffer: Rc<RefCell<WindowBuffer>>) -> PlotState {
        PlotState {
            enabled_signals: HashSet::new(),
            window_buffer,
        }
    }
}

pub struct MainViewState {
    connect_state: ConnectState,
    client: Option<Arc<Mutex<Box<dyn Client + Send>>>>,
    selected_plot_id: Arc<AtomicU32>,
    plots: HashMap<u32, PlotState>,

    signals_view_actions: LinkedList<SignalsViewAction>,
}

impl State<MainViewAction> for MainViewState {
    fn apply(&mut self, action: MainViewAction) {
        match action {
            // Connection
            MainViewAction::Connect(port) => self.connect(port),
            MainViewAction::ConnectSuccess(mut client) => {
                for (_, state) in &mut self.plots {
                    client.add_callback(state.window_buffer.borrow_mut().callback()).block_on();
                }

                self.client = Some(Arc::new(Mutex::new(client)));
                self.connect_state = ConnectState::Connected;
            }
            MainViewAction::ConnectFailed(err) => {
                println!("Connect failed: {err}");
                self.connect_state = ConnectState::Disconnected;
            }

            // Active plot
            MainViewAction::SetActivePlot(id) => {
                self.selected_plot_id.store(id, Ordering::SeqCst);
            }

            MainViewAction::AddSignalToCurrentPlot(signal_id) => {
                let plot_id = self.selected_plot_id.load(Ordering::SeqCst);
                self.plots.get_mut(&plot_id).unwrap().window_buffer.borrow_mut().add_signal(&signal_id);
            }
            MainViewAction::RemoveSignalFromCurrentPlot(signal_id) => {
                let plot_id = self.selected_plot_id.load(Ordering::SeqCst);
                self.plots.get_mut(&plot_id).unwrap().window_buffer.borrow_mut().remove_signal(&signal_id);
            }
        }
    }
}

impl MainViewState {
    pub fn new(selected_plot_id: Arc<AtomicU32>) -> MainViewState {
        MainViewState {
            connect_state: ConnectState::Disconnected,
            client: None,
            selected_plot_id,
            plots: Default::default(),

            signals_view_actions: Default::default(),
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

    fn add_plot(&mut self, plot_id: u32, buffer: Rc<RefCell<WindowBuffer>>) {
        self.plots.insert(plot_id, PlotState::new(buffer));
    }
}


pub struct MainView {
    state: MainViewState,

    connect_view: ConnectView,

    sidebar_settings: SidebarSettingsView,
    signals_view: Option<SignalsView>,

    plot_view: Vec<PlotView>,
}

impl MainView {
    pub fn new() -> MainView {
        let selected_plot_id = Arc::new(AtomicU32::new(1));
        let mut result = MainView {
            state: MainViewState::new(selected_plot_id.clone()),
            connect_view: ConnectView::new(),
            signals_view: None,
            sidebar_settings: SidebarSettingsView::new(),
            plot_view: vec![],
        };

        for i in [1u32, 2u32, 3u32, 4u32] {
            let window_buf = Rc::new(RefCell::new(WindowBuffer::new()));

            result.plot_view.push(PlotView::new(i, selected_plot_id.clone(), window_buf.clone()));
            result.state.add_plot(i, window_buf.clone());
        }

        result
    }
}

impl TopLevelView<MainViewState, MainViewAction> for MainView {
    fn state(&mut self) -> &mut MainViewState {
        &mut self.state
    }

    fn view(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> LinkedList<MainViewAction> {
        if let Some(sv) = &mut self.signals_view {
            sv.state().apply_all(&mut self.state.signals_view_actions);
        }

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
                result.append(&mut self.view_connected(ctx, frame));
            }
        }

        result
    }
}

impl MainView {
    fn view_disconnected(
        &mut self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) -> LinkedList<MainViewAction> {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.connect_view.render(ui).inner
        }).inner
    }

    fn view_connecting(
        &self,
        ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| ui.spinner());
        });
    }

    fn view_connected(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) -> LinkedList<MainViewAction> {
        if self.signals_view.is_none() {
            let mut signals_view = SignalsView::new(self.state.client.as_ref().unwrap().clone(), self.state.selected_plot_id.clone());
            signals_view.state().apply(SignalsViewAction::FetchSignals);
            self.signals_view = Some(signals_view);
        }

        let mut result = LinkedList::<MainViewAction>::default();

        let mut signals_view_actions = egui::SidePanel::left("signals")
            .exact_width(240.0)
            .show(ctx, |ui| {
                self.sidebar_settings.render(ui);
                ui.separator();
                self.signals_view.as_mut().unwrap().render(ui)
            }).inner;
        result.append(&mut signals_view_actions.inner);

        let size = ctx.available_rect();

        egui::CentralPanel::default()
            .show(ctx, |ui| {
                egui::Grid::new("plots").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                    ui.add_sized([size.width() / 2.0 - 12.0, size.height() / 2.0 - 12.0], |ui: &mut Ui| {
                        Self::render_plot(&mut self.plot_view[0], ui, &mut result)
                    });
                    ui.add_sized([size.width() / 2.0 - 12.0, size.height() / 2.0 - 12.0], |ui: &mut Ui| {
                        Self::render_plot(&mut self.plot_view[1], ui, &mut result)
                    });

                    ui.end_row();

                    ui.add_sized([size.width() / 2.0 - 12.0, size.height() / 2.0 - 12.0], |ui: &mut Ui| {
                        Self::render_plot(&mut self.plot_view[2], ui, &mut result)
                    });
                    ui.add_sized([size.width() / 2.0 - 12.0, size.height() / 2.0 - 12.0], |ui: &mut Ui| {
                        Self::render_plot(&mut self.plot_view[3], ui, &mut result)
                    });
                });
            });

        result
    }

    fn render_plot(plot: &mut PlotView, ui: &mut Ui, actions: &mut LinkedList<MainViewAction>) -> Response {
        let ir = plot.render(ui);

        for action in ir.inner {
            actions.push_back(match action {
                PlotViewParentAction::SetActivePlot(id) => MainViewAction::SetActivePlot(id)
            });
        }

        ir.response
    }
}
