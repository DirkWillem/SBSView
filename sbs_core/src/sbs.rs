use std::future::Future;
use crate::ty::Type;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameId(pub u32);


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

pub trait Client {
    type Error;

    fn get_frames(&mut self) -> impl Future<Output=Result<Vec<SignalFrameDescriptor>, Self::Error>> + Send;

    fn enable_frame(&mut self, frame_id: FrameId) -> impl Future<Output=Result<(), Self::Error>> + Send;
    fn disable_frame(&mut self, frame_id: FrameId) -> impl Future<Output=Result<(), Self::Error>> + Send;
}

