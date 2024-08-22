use std::collections::LinkedList;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use eframe::egui;
use eframe::egui::{CollapsingHeader, DragValue, InnerResponse, Ui};
use egui_plot::Plot;
use crate::view::{State, View};

pub enum PlotViewAction {
    ToggleSettings,
    MakeActive,
}

pub enum PlotViewParentAction {
    SetActivePlot(u32)
}

pub struct PlotViewState {
    show_settings: bool,
    window: f32,
    id: u32,
    active_id: Arc<AtomicU32>,
}

impl State<PlotViewAction> for PlotViewState {
    fn apply(&mut self, action: PlotViewAction) {
        match action {
            PlotViewAction::ToggleSettings => self.show_settings = !self.show_settings,
            PlotViewAction::MakeActive => {}
        }
    }
}

impl PlotViewState {
    fn new(id: u32, active_id: Arc<AtomicU32>) -> PlotViewState {
        PlotViewState {
            show_settings: false,
            window: 10.0,
            id,
            active_id,
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

            plot.show(ui, |plot_ui| {});

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
    pub fn new(id: u32, active_id: Arc<AtomicU32>) -> PlotView {
        PlotView {
            state: PlotViewState::new(id, active_id),
            plot_id: format!("plot_{id}"),
            settings_id: format!("plot_settings_{id}"),
        }
    }
}


