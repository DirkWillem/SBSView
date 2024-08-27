use crate::signals::window_buffer::{Snapshot, WindowBuffer};
use crate::view::{State, View};
use eframe::egui;
use eframe::egui::{DragValue, InnerResponse, Ui};
use egui_plot::{Line, Plot, PlotPoints};
use std::cell::RefCell;
use std::collections::LinkedList;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

pub enum PlotViewAction {
    ToggleSettings,
    MakeActive,
    TakeSnapshot,
    UpdateSnapshot(Snapshot),
}

pub enum PlotViewParentAction {
    SetActivePlot(u32)
}

pub enum SnapshotState {
    Idle,
    TakingSnapshot,
}

pub struct PlotViewState {
    show_settings: bool,
    window: f32,
    id: u32,
    active_id: Arc<AtomicU32>,
    buf: Rc<RefCell<WindowBuffer>>,
    buf_snapshot: Snapshot,
    snapshot_state: SnapshotState,
    last_snapshot_at: SystemTime,
}

impl State<PlotViewAction> for PlotViewState {
    fn apply(&mut self, action: PlotViewAction) {
        match action {
            PlotViewAction::ToggleSettings => self.show_settings = !self.show_settings,
            PlotViewAction::MakeActive => {}
            PlotViewAction::TakeSnapshot => {
                self.buf.borrow_mut().request_snapshot();
                self.snapshot_state = SnapshotState::TakingSnapshot;
            }
            PlotViewAction::UpdateSnapshot(snapshot) => {
                self.buf_snapshot = snapshot;
                self.last_snapshot_at = SystemTime::now();
                println!("{:?}", self.buf_snapshot);
                self.snapshot_state = SnapshotState::Idle;
            }
        }
    }

    fn poll_effects(&mut self) -> LinkedList<PlotViewAction> {
        match self.snapshot_state {
            SnapshotState::Idle =>
                if SystemTime::now().duration_since(self.last_snapshot_at).unwrap().as_millis() > 50 {
                    [PlotViewAction::TakeSnapshot].into()
                } else {
                    Default::default()
                }
            SnapshotState::TakingSnapshot =>
                if let Some(snapshot) = self.buf.borrow_mut().poll_snapshot() {
                    [PlotViewAction::UpdateSnapshot(snapshot)].into()
                } else {
                    Default::default()
                }
        }
    }
}

impl PlotViewState {
    fn new(id: u32, active_id: Arc<AtomicU32>, buf: Rc<RefCell<WindowBuffer>>) -> PlotViewState {
        PlotViewState {
            show_settings: false,
            window: 10.0,
            id,
            active_id,
            buf,
            buf_snapshot: Default::default(),
            snapshot_state: SnapshotState::Idle,
            last_snapshot_at: SystemTime::now(),
        }
    }
}

pub struct PlotView {
    state: PlotViewState,
    plot_id: String,
    settings_id: String,
}

impl View<PlotViewState, PlotViewAction, PlotViewParentAction> for PlotView {
    fn state(&mut self) -> &mut PlotViewState {
        &mut self.state
    }

    fn view(&mut self, ui: &mut Ui) -> InnerResponse<LinkedList<PlotViewAction>> {
        let mut result = LinkedList::<PlotViewAction>::new();
        let plot = Plot::new(&self.plot_id)
            .show_axes(true)
            .show_grid(true);

        ui.with_layout(egui::Layout::top_down(egui::Align::Center).with_cross_justify(false).with_main_align(egui::Align::TOP), |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.state.show_settings, "⛭").clicked() {
                    result.push_back(PlotViewAction::ToggleSettings)
                }
                if ui.selectable_label(self.state.id == self.state.active_id.load(Ordering::SeqCst), format!("Plot {}", self.state.id)).clicked() {
                    result.push_back(PlotViewAction::MakeActive)
                }
                ui.label(format!("Window: {} s", self.state.window))
            });

            if self.state.show_settings {
                ui.group(|ui| {
                    egui::Grid::new(&self.settings_id)
                        .num_columns(2)
                        .spacing([40.0, 0.0])
                        .striped(true).show(ui, |ui| {
                        ui.label("Window");
                        ui.add(DragValue::new(&mut self.state.window)
                            .range(1.0..=100.0)
                            .speed(0.5));
                        ui.end_row();
                    });
                });
            }

            ui.ctx().request_repaint();

            plot.show(ui, |plot_ui| {
                for ((_, name), values) in &self.state.buf_snapshot {
                    plot_ui.line(Line::new(PlotPoints::from_iter(values.iter().map(|(t, v)| [*t as f64, v.clone().into()]))).name(name));
                }
            });

            result
        })
    }

    fn action_to_parent_action(&self, action: &PlotViewAction) -> Option<PlotViewParentAction> {
        match action {
            PlotViewAction::MakeActive => Some(PlotViewParentAction::SetActivePlot(self.state.id)),
            _ => None,
        }
    }
}

impl PlotView {
    pub fn new(id: u32, active_id: Arc<AtomicU32>, buf: Rc<RefCell<WindowBuffer>>) -> PlotView {
        PlotView {
            state: PlotViewState::new(id, active_id, buf),
            plot_id: format!("plot_{id}"),
            settings_id: format!("plot_settings_{id}"),
        }
    }
}


