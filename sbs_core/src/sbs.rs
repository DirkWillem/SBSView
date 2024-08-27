use crate::ty::Type;
use crate::value::SignalFrameValue;
use async_trait::async_trait;
use std::fmt::Debug;
use std::future::Future;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameId(pub u32);

pub type SignalId = (FrameId, String);


#[derive(Clone, Debug)]
pub struct SignalFrameDescriptor {
    pub id: FrameId,
    pub name: String,
    pub enabled: bool,
    pub signals: Vec<SignalDescriptor>,
}

#[derive(Clone, Debug)]
pub struct SignalDescriptor {
    pub name: String,
    pub ty: Type,
}

pub trait SignalFrameCallback: Fn(FrameId, &SignalFrameValue) + Send + Sync {}

impl<T> SignalFrameCallback for T
where
    T: Fn(FrameId, &SignalFrameValue) + Send + Sync,
{}

#[async_trait]
pub trait Client {
    async fn get_frames(&mut self) -> Result<Vec<SignalFrameDescriptor>, String>;

    async fn enable_frame(&mut self, frame_id: FrameId) -> Result<(), String>;
    async fn disable_frame(&mut self, frame_id: FrameId) -> Result<(), String>;

    async fn add_callback(&mut self, cb: Box<dyn SignalFrameCallback>);
}

