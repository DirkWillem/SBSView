use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{mpsc, RwLock};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use sbs_core::sbs::{Client, SignalFrameDescriptor, FrameId, SignalDescriptor, SignalFrameCallback};
use sbs_core::value::SignalFrameValue;
use crate::error::Error;
use crate::frame_decoder::RawSignalFrame;
use crate::serial_worker::SerialWorker;

struct FrameState {
    descriptor: SignalFrameDescriptor,
    latest_value: SignalFrameValue,
}

pub struct SbsUart {
    serial_worker: SerialWorker,
    frame_descriptors: Arc<RwLock<Option<HashMap<FrameId, FrameState>>>>,
    #[allow(dead_code)]
    frame_reader_thread: JoinHandle<()>,

    callbacks: Arc<RwLock<Vec<Box<dyn SignalFrameCallback>>>>,
}


#[async_trait]
impl Client for SbsUart {
    async fn get_frames(&mut self) -> Result<Vec<SignalFrameDescriptor>, String> {
        self.ensure_frame_descriptors_loaded().await?;

        let mut frames = self.frame_descriptors.read().await.as_ref().unwrap()
            .values()
            .map(|fs| fs.descriptor.clone()).collect::<Vec<_>>();
        frames.sort_by(|a, b| a.id.0.cmp(&b.id.0));

        Ok(frames)
    }

    async fn enable_frame(&mut self, frame_id: FrameId) -> Result<(), String> {
        self.serial_worker.enable_frame(frame_id.0).await?;

        if let Some(ref mut descriptors) = &mut *self.frame_descriptors.write().await {
            if let Some(entry) = descriptors.get_mut(&frame_id) {
                entry.descriptor.enabled = true;
            }
        }

        Ok(())
    }

    async fn disable_frame(&mut self, frame_id: FrameId) -> Result<(), String> {
        self.serial_worker.disable_frame(frame_id.0).await?;

        if let Some(ref mut descriptors) = &mut *self.frame_descriptors.write().await {
            if let Some(entry) = descriptors.get_mut(&frame_id) {
                entry.descriptor.enabled = false;
            }
        }

        Ok(())
    }

    async fn add_callback(&mut self, cb: Box<dyn SignalFrameCallback>) {
        let mut cbs = self.callbacks.write().await;
        (*cbs).push(cb);
    }
}

impl SbsUart {
    pub fn new() -> SbsUart {
        let (raw_frame_tx, mut raw_frame_rx): (Sender<RawSignalFrame>, Receiver<RawSignalFrame>) = mpsc::channel(32);

        let frame_descriptors = Arc::new(RwLock::new(None));
        let callbacks = Arc::new(RwLock::new(Vec::<Box<dyn SignalFrameCallback>>::new()));

        SbsUart {
            serial_worker: SerialWorker::new(raw_frame_tx),
            frame_descriptors: Arc::clone(&frame_descriptors),
            callbacks: callbacks.clone(),
            frame_reader_thread: tokio::spawn(async move {
                let descriptors_rwl = frame_descriptors.clone();
                let callbacks = callbacks.clone();
                while let Some(frame) = raw_frame_rx.recv().await {
                    let frame_id = FrameId(frame.frame_id);

                    let mut descriptors_opt = descriptors_rwl.write().await;
                    if let Some(ref mut descriptors) = &mut *descriptors_opt {
                        if let Some(frame_state) = descriptors.get_mut(&frame_id) {
                            frame_state.latest_value.update_from_bytes(frame.timestamp, frame.data.as_slice());

                            for cb in callbacks.read().await.iter() {
                                (*cb)(frame_id, &frame_state.latest_value);
                            }
                        }
                    }
                }
            }),
        }
    }

    pub async fn connect(&mut self, port: &str, baud: u32) -> Result<(), Error> {
        self.serial_worker.connect(port, baud).await
    }

    async fn ensure_frame_descriptors_loaded(&mut self) -> Result<(), Error> {
        let mut result = HashMap::<FrameId, FrameState>::new();
        let frames = self.serial_worker.list_frames().await?;

        for frame in frames {
            let frame_details = self.serial_worker.get_frame_info(frame.id).await?;

            let descriptor = SignalFrameDescriptor {
                id: FrameId(frame.id),
                name: frame.name.clone(),
                enabled: frame_details.enabled,
                signals: frame_details.signals.iter().map(|s| SignalDescriptor {
                    name: s.name.clone(),
                    ty: s.ty.clone(),
                }).collect::<Vec<_>>(),
            };

            result.insert(FrameId(frame.id), FrameState {
                descriptor: descriptor.clone(),
                latest_value: SignalFrameValue::new(descriptor.clone()),
            });
        }

        let mut descriptors = self.frame_descriptors.write().await;
        *descriptors = Some(result);
        Ok(())
    }
}
