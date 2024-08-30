use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::thread::JoinHandle;
use pollster::FutureExt;
use tokio::sync::RwLock;
use sbs_core::sbs::{FrameId, SignalFrameCallback, SignalId};
use sbs_core::value::{SignalFrameValue, Value};

pub type Snapshot = HashMap<SignalId, VecDeque<(u32, Value)>>;

enum Cmd {
    SetWindow(f32),
    AddSignal(SignalId),
    RemoveSignal(SignalId),
    ProcessFrame(FrameId, SignalFrameValue),
    TakeSnapshot,
    Quit,
}

pub struct WindowBuffer {
    signals_buffer: Arc<RwLock<HashMap<SignalId, VecDeque<(u32, Value)>>>>,
    snapshot_ready: Arc<AtomicBool>,
    rw_thread: JoinHandle<()>,
    cmd_tx: mpsc::Sender<Cmd>,
    snapshot_rx: mpsc::Receiver<Snapshot>,
}


impl Drop for WindowBuffer {
    fn drop(&mut self) {
        self.cmd_tx.send(Cmd::Quit).expect("Failed to send Cmd");
    }
}

impl WindowBuffer {
    pub fn new() -> WindowBuffer {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (snapshot_tx, snapshot_rx) = mpsc::channel();

        WindowBuffer {
            signals_buffer: Arc::new(RwLock::new(HashMap::new())),
            rw_thread: thread::spawn(move || {
                let mut window: u32 = 10_000;
                let mut buf = Snapshot::default();

                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        Cmd::SetWindow(new_window) => {
                            window = (new_window * 1000.0) as u32;
                            println!("{window}");
                        },
                        Cmd::AddSignal(signal_id) =>
                            if !buf.contains_key(&signal_id) {
                                buf.insert(signal_id, VecDeque::new());
                            }
                        Cmd::RemoveSignal(signal_id) =>
                            if buf.contains_key(&signal_id) {
                                buf.remove(&signal_id);
                            },
                        Cmd::ProcessFrame(frame_id, value) => {
                            for (i, descriptor) in value.descriptor.signals.iter().enumerate() {
                                let signal_id = (frame_id, descriptor.name.clone());

                                if let Some(sig_buf) = buf.get_mut(&signal_id) {
                                    sig_buf.push_back((value.timestamp, value.data[i].clone()));

                                    while let Some((ts, _)) = sig_buf.front() {
                                        if (value.timestamp - ts) > window {
                                            sig_buf.pop_front();
                                        } else {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Cmd::TakeSnapshot => {
                            let snapshot = buf.clone();
                            snapshot_tx.send(snapshot).expect("Failed to send snapshot");
                        }
                        Cmd::Quit => break
                    }
                }
            }),
            snapshot_ready: Arc::new(AtomicBool::new(false)),
            cmd_tx,
            snapshot_rx,
        }
    }

    pub fn callback(&self) -> Box<dyn SignalFrameCallback> {
        Box::new({
            let cmd_tx = self.cmd_tx.clone();
            move |frame_id: FrameId, value: &SignalFrameValue| {
                cmd_tx.send(Cmd::ProcessFrame(frame_id, value.clone())).expect("Failed to send signal");
            }
        })
    }

    pub fn add_signal(&mut self, signal_id: &SignalId) {
        self.cmd_tx.send(Cmd::AddSignal(signal_id.clone())).expect("Failed to send Cmd");
    }

    pub fn remove_signal(&mut self, signal_id: &SignalId) {
        self.cmd_tx.send(Cmd::RemoveSignal(signal_id.clone())).expect("Failed to send Cmd");
    }

    pub fn set_window(&mut self, window: f32) {
        self.cmd_tx.send(Cmd::SetWindow(window)).expect("Failed to send Cmd");
    }

    pub fn request_snapshot(&mut self) {
        self.cmd_tx.send(Cmd::TakeSnapshot).expect("Failed to send Cmd");
    }

    pub fn poll_snapshot(&mut self) -> Option<Snapshot> {
        if let Ok(snapshot) = self.snapshot_rx.try_recv() {
            Some(snapshot)
        } else {
            None
        }
    }
}