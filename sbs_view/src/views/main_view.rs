use eframe::egui;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, LinkedList};
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use eframe::egui::{ComboBox, Response, Ui};
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

#[derive(PartialEq)]
pub enum PlotsLayout {
    Single,
    TwoHorizontal,
    TwoVertical,
    TwoByTwoGrid,
}

impl Display for PlotsLayout {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PlotsLayout::Single => write!(f, "Single"),
            PlotsLayout::TwoHorizontal => write!(f, "2 Split Horizontal"),
            PlotsLayout::TwoVertical => write!(f, "2 Split Vertical"),
            PlotsLayout::TwoByTwoGrid => write!(f, "2x2 Grid"),
        }
    }
}

pub enum MainViewAction {
    SetActivePlot(u32),

    Connect(Port),
    ConnectSuccess(Box<dyn Client + Send>),
    ConnectFailed(String),

    AddSignalToCurrentPlot(SignalId),
    RemoveSignalFromCurrentPlot(SignalId),

    SetPlotWindow(u32, f32),

    SetLayout(PlotsLayout),
}

enum ConnectState {
    Disconnected,
    Connecting(AsyncProcess<Result<Box<SbsUart>, String>>),
    Connected,
}

struct PlotState {
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
    view_layout: PlotsLayout,

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

            // Plot settings
            MainViewAction::SetPlotWindow(id, window) => {
                self.plots.get_mut(&id).unwrap().window_buffer.borrow_mut().set_window(window);
            }

            // Layout
            MainViewAction::SetLayout(layout) => {
                self.view_layout = layout;
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
            view_layout: PlotsLayout::Single,

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

    fn ensure_views_exist(&mut self) {
        match self.state.view_layout {
            PlotsLayout::Single => self.ensure_n_views_exist(1),
            PlotsLayout::TwoHorizontal | PlotsLayout::TwoVertical => self.ensure_n_views_exist(2),
            PlotsLayout::TwoByTwoGrid => self.ensure_n_views_exist(4),
        }
    }

    fn ensure_n_views_exist(&mut self, n: usize) {
        for i in 1..=n {
            if i > self.plot_view.len() {
                let window_buf = Rc::new(RefCell::new(WindowBuffer::new()));
                self.plot_view.push(PlotView::new(i as u32, self.state.selected_plot_id.clone(), window_buf.clone()));
                self.state.add_plot(i as u32, window_buf.clone());
            }
        }
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
                ComboBox::from_id_source("Layout").selected_text(self.state.view_layout.to_string()).show_ui(ui, |ui| {
                    for layout in [
                        PlotsLayout::Single,
                        PlotsLayout::TwoHorizontal,
                        PlotsLayout::TwoVertical,
                        PlotsLayout::TwoByTwoGrid,
                    ] {
                        if ui.selectable_label(self.state.view_layout == layout, format!("{layout}")).clicked() {
                            result.push_back(MainViewAction::SetLayout(layout));
                        }
                    }
                });

                ui.separator();
                self.signals_view.as_mut().unwrap().render(ui)
            }).inner;
        result.append(&mut signals_view_actions.inner);

        let size = ctx.available_rect();


        self.ensure_views_exist();
        let (nx, ny): (usize, usize) = match self.state.view_layout {
            PlotsLayout::Single => (1, 1),
            PlotsLayout::TwoHorizontal => (2, 1),
            PlotsLayout::TwoVertical => (1, 2),
            PlotsLayout::TwoByTwoGrid => (2, 2),
        };


        egui::CentralPanel::default()
            .show(ctx, |ui| {
                egui::Grid::new("plots").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                    let size_x = size.width() / (nx as f32) - (8.0 + 4.0 * (nx as f32));
                    let size_y = size.height() / (ny as f32) - (8.0 + 4.0 * (ny as f32));

                    for iy in 0..ny {
                        for ix in 0..nx {
                            let i = iy * nx + ix;

                            ui.add_sized([size_x, size_y], |ui: &mut Ui| {
                                Self::render_plot(&mut self.plot_view[i], ui, &mut result)
                            });
                        }

                        ui.end_row();
                    }
                });
            });

        result
    }

    fn render_plot(plot: &mut PlotView, ui: &mut Ui, actions: &mut LinkedList<MainViewAction>) -> Response {
        let ir = plot.render(ui);

        for action in ir.inner {
            actions.push_back(match action {
                PlotViewParentAction::SetActivePlot(id) => MainViewAction::SetActivePlot(id),
                PlotViewParentAction::SetWindow(window) => MainViewAction::SetPlotWindow(plot.id(), window),
            });
        }

        ir.response
    }
}
