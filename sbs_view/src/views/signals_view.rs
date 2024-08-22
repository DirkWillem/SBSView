use std::collections::{HashMap, HashSet, LinkedList};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use eframe::egui;
use eframe::egui::{InnerResponse, Ui};
use tokio::sync::Mutex;
use sbs_core::sbs::{Client, FrameId, SignalFrameDescriptor, SignalId};
use crate::view::{AsyncProcess, State, View};
use crate::views::main_view::MainViewAction;

pub enum SignalsViewAction {
    FetchSignals,
    FetchSignalsSuccess(Vec<SignalFrameDescriptor>),
    FetchSignalsFailed(String),

    EnableSignal(SignalId),
    EnableSignalSuccess(Vec<SignalFrameDescriptor>, SignalId),
    EnableSignalFailed(String),

    DisableSignal(SignalId),
    DisableSignalSuccess(Vec<SignalFrameDescriptor>),
    DisableSignalFailed(String),
}

pub enum Signals {
    Initial,
    Loading(AsyncProcess<Result<Vec<SignalFrameDescriptor>, String>>),
    Loaded(Vec<SignalFrameDescriptor>),
    Error(String),
}

pub enum EnableState {
    Idle,
    EnablingSignal(AsyncProcess<Result<Vec<SignalFrameDescriptor>, String>>, SignalId),
    DisablingSignal(AsyncProcess<Result<Vec<SignalFrameDescriptor>, String>>, SignalId),
}


pub struct SignalsViewState {
    client: Arc<Mutex<Box<dyn Client + Send>>>,
    signals: Signals,
    enable_state: EnableState,
    enabled_signals: HashMap<(FrameId, String), HashSet<u32>>,
    active_plot_id: Arc<AtomicU32>,
}

impl State<SignalsViewAction> for SignalsViewState {
    fn apply(&mut self, action: SignalsViewAction) {
        match action {
            SignalsViewAction::FetchSignals =>
                self.signals = Signals::Loading(AsyncProcess::<Result<Vec<SignalFrameDescriptor>, String>>::new({
                    let client_mtx = self.client.clone();
                    async move {
                        let mut client = client_mtx.lock().await;
                        client.get_frames().await
                    }
                })),
            SignalsViewAction::FetchSignalsSuccess(signals) => {
                self.signals = Signals::Loaded(signals)
            }
            SignalsViewAction::FetchSignalsFailed(errmsg) =>
                {
                    self.signals = Signals::Error(errmsg)
                }

            SignalsViewAction::EnableSignal(signal_id) => {
                assert!(matches!(self.enable_state, EnableState::Idle));

                // Check if the frame is enabled
                if self.frame_is_enabled(signal_id.0) {
                    self.enable_signal(&signal_id);
                } else {
                    let enable_proc = AsyncProcess::<Result<Vec<SignalFrameDescriptor>, String>>::new({
                        let client_mtx = self.client.clone();
                        async move {
                            let mut client = client_mtx.lock().await;
                            client.enable_frame(signal_id.0).await?;
                            client.get_frames().await
                        }
                    });
                    self.enable_state = EnableState::EnablingSignal(enable_proc, signal_id);
                }
            }
            SignalsViewAction::EnableSignalSuccess(new_frames, signal_id) => {
                self.enable_signal(&signal_id);
                self.signals = Signals::Loaded(new_frames);
                self.enable_state = EnableState::Idle;
            }
            SignalsViewAction::EnableSignalFailed(err) => {
                println!("{err}");
                self.enable_state = EnableState::Idle;
            }

            SignalsViewAction::DisableSignal(signal_id) => {
                self.disable_signal(&signal_id);

                if !self.frame_has_enabled_signals(signal_id.0) {
                    let disable_proc = AsyncProcess::<Result<Vec<SignalFrameDescriptor>, String>>::new({
                        let client_mtx = self.client.clone();
                        async move {
                            let mut client = client_mtx.lock().await;
                            client.disable_frame(signal_id.0).await?;
                            client.get_frames().await
                        }
                    });

                    self.enable_state = EnableState::DisablingSignal(disable_proc, signal_id);
                }
            }
            SignalsViewAction::DisableSignalSuccess(new_frames) => {
                self.signals = Signals::Loaded(new_frames);
                self.enable_state = EnableState::Idle;
            }
            SignalsViewAction::DisableSignalFailed(err) => {
                println!("{err}");
                self.enable_state = EnableState::Idle;
            }
        }
    }

    fn poll_effects(&mut self) -> LinkedList<SignalsViewAction> {
        let mut result = LinkedList::<SignalsViewAction>::new();

        if let Signals::Loading(ref mut proc) = self.signals {
            if proc.is_done() {
                result.push_back(match proc.get() {
                    Ok(signals) => SignalsViewAction::FetchSignalsSuccess(signals),
                    Err(err) => SignalsViewAction::FetchSignalsFailed(err),
                })
            }
        }

        match &mut self.enable_state {
            EnableState::Idle => {}
            EnableState::EnablingSignal(ref mut proc, signal_id) => if proc.is_done() {
                result.push_back(match proc.get() {
                    Ok(frames) => {
                        SignalsViewAction::EnableSignalSuccess(frames, signal_id.clone())
                    }
                    Err(err) => SignalsViewAction::EnableSignalFailed(err)
                })
            },
            EnableState::DisablingSignal(ref mut proc, _signal_id) => if proc.is_done() {
                result.push_back(match proc.get() {
                    Ok(frames) => SignalsViewAction::DisableSignalSuccess(frames),
                    Err(err) => SignalsViewAction::DisableSignalFailed(err),
                })
            }
        }

        result
    }
}

impl SignalsViewState {
    pub fn new(
        client: Arc<Mutex<Box<dyn Client + Send>>>,
        active_plot_id: Arc<AtomicU32>,
    ) -> SignalsViewState {
        SignalsViewState {
            client,
            signals: Signals::Initial,
            enable_state: EnableState::Idle,
            enabled_signals: Default::default(),
            active_plot_id,
        }
    }

    fn frame_is_enabled(&self, id: FrameId) -> bool {
        if let Signals::Loaded(frames) = &self.signals {
            frames.iter()
                .find(|frame| frame.id == id)
                .map(|frame| frame.enabled)
                .unwrap_or(false)
        } else {
            false
        }
    }

    fn enable_signal(&mut self, signal_id: &SignalId) {
        let active_id = self.active_plot_id.load(Ordering::SeqCst);

        if !self.enabled_signals.contains_key(signal_id) {
            self.enabled_signals.insert(signal_id.clone(), [active_id].into());
        } else {
            self.enabled_signals.get_mut(&signal_id).unwrap().insert(active_id);
        }
    }

    fn disable_signal(&mut self, signal_id: &SignalId) {
        let active_id = self.active_plot_id.load(Ordering::SeqCst);

        if let Some(plot_ids) = self.enabled_signals.get_mut(signal_id) {
            plot_ids.remove(&active_id);
        }
    }

    fn signal_enabled_for_current_plot(&self, signal_id: &SignalId) -> bool {
        let active_id = self.active_plot_id.load(Ordering::SeqCst);

        self.enabled_signals
            .get(signal_id)
            .map(|plot_ids| plot_ids.contains(&active_id))
            .unwrap_or(false)
    }

    fn frame_has_enabled_signals(&self, frame_id: FrameId) -> bool {
        self.enabled_signals
            .iter()
            .any(|((fid, _), v)| fid.eq(&frame_id) && !v.is_empty())
    }
}

pub struct SignalsView {
    state: SignalsViewState,
}

impl View<SignalsViewState, SignalsViewAction, MainViewAction> for SignalsView {
    fn state(&mut self) -> &mut SignalsViewState {
        &mut self.state
    }

    fn view(&mut self, ui: &mut Ui) -> InnerResponse<LinkedList<SignalsViewAction>> {
        let result = LinkedList::<SignalsViewAction>::new();
        match &self.state.signals {
            Signals::Initial | Signals::Loading(_) => {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                    result
                })
            }
            Signals::Loaded(frames) => {
                self.signals_tree(frames, ui)
            }
            Signals::Error(err) => {
                InnerResponse::new(result, ui.label(format!("Failed to load signals: {err}")))
            }
        }
    }
}

impl SignalsView {
    pub fn new(client: Arc<Mutex<Box<dyn Client + Send>>>, active_plot_id: Arc<AtomicU32>) -> SignalsView {
        SignalsView {
            state: SignalsViewState::new(client, active_plot_id),
        }
    }

    fn signals_tree(
        &self,
        frames: &Vec<SignalFrameDescriptor>,
        ui: &mut Ui,
    ) -> InnerResponse<LinkedList<SignalsViewAction>> {
        ui.vertical(|ui| {
            let mut result = LinkedList::<SignalsViewAction>::new();

            for frame in frames {
                let name = if frame.enabled {
                    format!("{} (enabled)", frame.name)
                } else {
                    format!("{}", frame.name)
                };

                egui::CollapsingHeader::new(name)
                    .id_source(frame.id.clone())
                    .default_open(true)
                    .show(ui, |ui| {
                        for signal in &frame.signals {
                            let signal_id = (frame.id, signal.name.clone());
                            let signal_enabled = self.state.signal_enabled_for_current_plot(&signal_id);

                            ui.horizontal(|ui| {
                                let busy = match &self.state.enable_state {
                                    EnableState::EnablingSignal(_, id) | EnableState::DisablingSignal(_, id) => id.eq(&signal_id),
                                    _ => false,
                                };

                                if busy {
                                    ui.spinner();
                                } else if !signal_enabled {
                                    if ui.selectable_label(false, "+").clicked() {
                                        result.push_back(SignalsViewAction::EnableSignal(signal_id));
                                    }
                                } else {
                                    if ui.selectable_label(true, "-").clicked() {
                                        result.push_back(SignalsViewAction::DisableSignal(signal_id));
                                    }
                                }


                                ui.label(&signal.name);
                            });
                        }
                    });
            }

            result
        })
    }
}
