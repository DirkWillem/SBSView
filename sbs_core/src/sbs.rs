use std::fmt::Debug;
use async_trait::async_trait;
use crate::ty::Type;

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

#[async_trait]
pub trait Client {
    async fn get_frames(&mut self) -> Result<Vec<SignalFrameDescriptor>, String>;

    async fn enable_frame(&mut self, frame_id: FrameId) -> Result<(), String>;
    async fn disable_frame(&mut self, frame_id: FrameId) -> Result<(), String>;
}

