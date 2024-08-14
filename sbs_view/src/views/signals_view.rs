use std::collections::LinkedList;
use std::sync::Arc;
use eframe::egui;
use eframe::egui::ahash::{HashMap, HashSet};
use eframe::egui::Ui;
use tokio::sync::Mutex;
use sbs_core::sbs::{Client, FrameId, SignalFrameDescriptor};
use crate::view::{AsyncProcess, State, View};
use crate::views::main_view::MainViewAction;

type SignalId = (FrameId, String);

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
    enabled_signals: HashSet<(FrameId, String)>,
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

            SignalsViewAction::EnableSignal((frame_id, signal_name)) => {
                assert!(matches!(self.enable_state, EnableState::Idle));

                // Check if the frame is enabled
                if self.frame_is_enabled(frame_id) {
                    self.enabled_signals.insert((frame_id, signal_name));
                } else {
                    let enable_proc = AsyncProcess::<Result<Vec<SignalFrameDescriptor>, String>>::new({
                        let client_mtx = self.client.clone();
                        async move {
                            let mut client = client_mtx.lock().await;
                            client.enable_frame(frame_id).await?;
                            client.get_frames().await
                        }
                    });
                    self.enable_state = EnableState::EnablingSignal(enable_proc, (frame_id, signal_name));
                }
            }
            SignalsViewAction::EnableSignalSuccess(new_frames, signal_id) => {
                self.enabled_signals.insert(signal_id);
                self.signals = Signals::Loaded(new_frames);
                self.enable_state = EnableState::Idle;
            }
            SignalsViewAction::EnableSignalFailed(err) => {
                println!("{err}");
                self.enable_state = EnableState::Idle;
            }

            SignalsViewAction::DisableSignal((frame_id, signal_name)) => {
                self.enabled_signals.remove(&(frame_id, signal_name.clone()));

                // Check if the message should be enabled
                if !self.enabled_signals
                    .iter()
                    .any(|(frame_id, _)| frame_id.eq(&frame_id)) {
                    let disable_proc = AsyncProcess::<Result<Vec<SignalFrameDescriptor>, String>>::new({
                        let client_mtx = self.client.clone();
                        async move {
                            let mut client = client_mtx.lock().await;
                            client.disable_frame(frame_id).await?;
                            client.get_frames().await
                        }
                    });

                    self.enable_state = EnableState::DisablingSignal(disable_proc, (frame_id, signal_name));
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

            _ => {}
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
            EnableState::DisablingSignal(ref mut proc, signal_id) => if proc.is_done() {
                result.push_back(match proc.get() {
                    Ok(frames) => SignalsViewAction::DisableSignalSuccess(frames),
                    Err(err) => SignalsViewAction::DisableSignalFailed(err),
                })
            }
            _ => {}
        }

        result
    }
}

impl SignalsViewState {
    pub fn new(client: Arc<Mutex<Box<dyn Client + Send>>>) -> SignalsViewState {
        SignalsViewState {
            client,
            signals: Signals::Initial,
            enable_state: EnableState::Idle,
            enabled_signals: Default::default(),
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
}

pub struct SignalsView {
    state: SignalsViewState,
}

impl View<SignalsViewState, SignalsViewAction, MainViewAction> for SignalsView {
    fn state(&mut self) -> &mut SignalsViewState {
        &mut self.state
    }

    fn view(&mut self, ui: &mut Ui) -> LinkedList<SignalsViewAction> {
        let mut result = LinkedList::<SignalsViewAction>::new();
        match &self.state.signals {
            Signals::Initial | Signals::Loading(_) => {
                ui.centered_and_justified(|ui| ui.spinner());
            }
            Signals::Loaded(frames) => {
                result.append(&mut self.signals_tree(frames, ui));
            }
            Signals::Error(err) => {
                ui.label(format!("Failed to load signals: {err}"));
            }
        }

        result
    }
}

impl SignalsView {
    pub fn new(client: Arc<Mutex<Box<dyn Client + Send>>>) -> SignalsView {
        let mut result = SignalsView {
            state: SignalsViewState::new(client),
        };


        result
    }

    fn signals_tree(
        &self,
        frames: &Vec<SignalFrameDescriptor>,
        ui: &mut Ui,
    ) -> LinkedList<SignalsViewAction> {
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
                        let signal_enabled = self.state.enabled_signals.contains(&signal_id);

                        ui.horizontal(|ui| {
                            let busy = match &self.state.enable_state {
                                EnableState::EnablingSignal(_, id) | EnableState::DisablingSignal(_, id) => id.eq(&signal_id),
                                _ => false,
                            };

                            if busy {
                                ui.spinner();
                            } else if !signal_enabled {
                                if ui.button("+").clicked() {
                                    result.push_back(SignalsViewAction::EnableSignal(signal_id));
                                }
                            } else {
                                if ui.button("-").clicked() {
                                    result.push_back(SignalsViewAction::DisableSignal(signal_id));
                                }
                            }


                            ui.label(&signal.name);
                        });
                    }
                });
        }

        result
    }
}
