use std::collections::VecDeque;
use sbs_core::ty::{parse_type_name, Type};

#[derive(Clone, Debug)]
pub struct FrameInfo {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct SignalInfo {
    pub name: String,
    pub ty: Type,
}

#[derive(Clone, Debug)]
pub struct FrameDetails {
    pub enabled: bool,
    pub signals: Vec<SignalInfo>,
}

#[derive(Clone, Debug)]
pub enum DecodedFrame {
    ListFrames(Vec<FrameInfo>),
    GetFrameInfo(FrameDetails),
    EnableFrame,
    DisableFrame,
}

#[derive(Clone, Debug)]
pub enum DecodeResult {
    None,
    CmdFrame(DecodedFrame),
    SignalFrame(RawSignalFrame),
    Err(String),
}

#[derive(Clone, Debug)]
enum DecodeListFramesState {
    NumFrames,
    FrameId,
    FrameNameLen,
    FrameName(u8),
}

#[derive(Clone, Debug, Default)]
struct PartialListFrames {
    num_frames: u32,
    frame_id: u32,
    frames: Vec<FrameInfo>,
}

#[derive(Clone, Debug)]
enum DecodeGetFrameInfoState {
    IsEnabled,
    NumSignals,
    SignalNameLen,
    SignalName(u8),
    SignalTypeLen,
    SignalType(u8),
}

#[derive(Clone, Debug)]
enum DecodeDataFrameState {
    FrameId,
    Timestamp,
    DataLen,
    Data(u32),
}

#[derive(Clone, Debug, Default)]
pub struct RawSignalFrame {
    pub frame_id: u32,
    pub timestamp: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
struct PartialGetFrameInfo {
    enabled: bool,
    num_signals: u32,
    signal_name: String,
    signals: Vec<SignalInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PayloadType {
    ListFrames,
    GetFrameInfo,
    EnableFrame,
    DisableFrame,
    DataFrame,
    NullFrame,
}

#[derive(Clone, Debug)]
enum DecoderState {
    StartWord,
    FrameLength,
    PayloadStartChar,
    DataFrame(DecodeDataFrameState),
    ListFrames(DecodeListFramesState),
    GetFrameInfo(DecodeGetFrameInfoState),
    PayloadEndChar(PayloadType, u8),
    Crc(PayloadType),
    EndChar(PayloadType),
}

impl From<DecodeDataFrameState> for DecoderState {
    fn from(value: DecodeDataFrameState) -> Self {
        DecoderState::DataFrame(value)
    }
}

impl From<DecodeListFramesState> for DecoderState {
    fn from(value: DecodeListFramesState) -> Self {
        DecoderState::ListFrames(value)
    }
}

impl From<DecodeGetFrameInfoState> for DecoderState {
    fn from(value: DecodeGetFrameInfoState) -> Self {
        DecoderState::GetFrameInfo(value)
    }
}

#[derive(Debug)]
pub struct Decoder {
    state: DecoderState,
    buffer: VecDeque<u8>,
    offset: usize,
    frame_len: usize,
    frame_start_offset: usize,

    data_frame: RawSignalFrame,
    list_frames: PartialListFrames,
    get_frame_info: PartialGetFrameInfo,
}

const FRAME_START: u32 = 0xBBBBBBBB;
const FRAME_END: u8 = 0xEE;

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            state: DecoderState::StartWord,
            buffer: VecDeque::new(),
            offset: 0,
            frame_len: 0,
            frame_start_offset: 0,

            data_frame: Default::default(),
            list_frames: Default::default(),
            get_frame_info: Default::default(),
        }
    }

    pub fn add_data(&mut self, data: &[u8]) {
        self.buffer.extend(data.iter().copied());
        self.buffer.make_contiguous();
    }

    pub fn decode(&mut self) -> DecodeResult {
        let mut result = DecodeResult::None;

        loop {
            let mut clear_read = false;

            let cur_state = self.state.clone();

            let new_state = match cur_state {
                DecoderState::StartWord => self.peek_u32_le()
                    .map(|sc| match sc {
                        FRAME_START => {
                            self.consume_u32_le().unwrap();
                            DecoderState::FrameLength
                        }
                        _ => {
                            self.consume_u8().unwrap();
                            clear_read = true;
                            DecoderState::StartWord
                        }
                    }),
                DecoderState::FrameLength => self.consume_u32_le()
                    .map(|fl| {
                        self.frame_len = fl as usize;
                        self.frame_start_offset = self.offset;
                        DecoderState::PayloadStartChar
                    }),
                DecoderState::PayloadStartChar => self.consume_u8()
                    .map(|sc| match sc {
                        b's' => {
                            self.data_frame = Default::default();
                            DecodeDataFrameState::FrameId.into()
                        }
                        b'l' => {
                            self.list_frames = Default::default();
                            DecodeListFramesState::NumFrames.into()
                        }
                        b'i' => {
                            self.get_frame_info = Default::default();
                            DecodeGetFrameInfoState::IsEnabled.into()
                        }
                        b'e' => DecoderState::PayloadEndChar(PayloadType::EnableFrame, b'E'),
                        b'd' => DecoderState::PayloadEndChar(PayloadType::DisableFrame, b'D'),
                        _ => {
                            clear_read = true;
                            DecoderState::StartWord
                        }
                        b'(' => DecoderState::PayloadEndChar(PayloadType::NullFrame, b')'),
                    }),
                DecoderState::DataFrame(inner) =>
                    self.decode_data_frame(inner),
                DecoderState::ListFrames(inner) =>
                    self.decode_list_frames(inner),
                DecoderState::GetFrameInfo(inner) =>
                    match self.decode_get_frame_info(inner) {
                        Ok(state) => state,
                        Err(errmsg) => {
                            result = DecodeResult::Err(errmsg);
                            clear_read = true;
                            Some(DecoderState::StartWord)
                        }
                    }
                DecoderState::PayloadEndChar(pt, ec) => {
                    self.consume_u8().map(|ec2| {
                        if ec == ec2 {
                            DecoderState::Crc(pt)
                        } else {
                            result = DecodeResult::Err(format!("Invalid payload end char {ec2}"));
                            clear_read = true;
                            DecoderState::StartWord
                        }
                    })
                }
                DecoderState::Crc(pt) => {
                    self.consume_u16_le().map(|crc| {
                        let crc16 = crc::Crc::<u16>::new(&crc::CRC_16_ARC);
                        let crc_data = &self.buffer.as_slices().0[5..self.offset - 2];
                        let crc_calc = crc16.checksum(crc_data);

                        if crc == crc_calc {
                            DecoderState::EndChar(pt)
                        } else {
                            result = DecodeResult::Err("Invalid frame CRC".to_string());
                            clear_read = true;
                            DecoderState::StartWord
                        }
                    })
                }
                DecoderState::EndChar(pt) => {
                    self.consume_u8().map(|ec| match ec {
                        FRAME_END => {
                            clear_read = true;

                            result = match pt {
                                PayloadType::ListFrames => DecodeResult::CmdFrame(DecodedFrame::ListFrames(self.list_frames.frames.clone())),
                                PayloadType::GetFrameInfo => DecodeResult::CmdFrame(DecodedFrame::GetFrameInfo(FrameDetails {
                                    enabled: self.get_frame_info.enabled,
                                    signals: self.get_frame_info.signals.clone(),
                                })),
                                PayloadType::EnableFrame => DecodeResult::CmdFrame(DecodedFrame::EnableFrame),
                                PayloadType::DisableFrame => DecodeResult::CmdFrame(DecodedFrame::DisableFrame),
                                PayloadType::DataFrame => DecodeResult::SignalFrame(self.data_frame.clone()),
                                PayloadType::NullFrame => result.clone(),
                            };

                            DecoderState::StartWord
                        }
                        _ => {
                            result = DecodeResult::Err(format!("Invalid frame end character {ec}"));
                            clear_read = true;
                            DecoderState::StartWord
                        }
                    })
                }
            };

            if clear_read {
                self.clear_read();
            }

            match new_state {
                Some(ns) => { self.state = ns; }
                None => break
            }

            if result.is_some() {
                return result;
            } else if !matches!(self.state, DecoderState::StartWord | DecoderState::FrameLength | DecoderState::Crc(_) | DecoderState::EndChar(_)) && self.offset >= (self.frame_start_offset + self.frame_len) {
                dbg!("Framelen exceeded");
            }
        }

        result
    }

    fn decode_data_frame(&mut self, inner: DecodeDataFrameState) -> Option<DecoderState> {
        match inner {
            DecodeDataFrameState::FrameId => self.consume_u32_le()
                .map(|fid| {
                    self.data_frame.frame_id = fid;
                    DecodeDataFrameState::Timestamp.into()
                }),
            DecodeDataFrameState::Timestamp => self.consume_u32_le()
                .map(|ts| {
                    self.data_frame.timestamp = ts;
                    DecodeDataFrameState::DataLen.into()
                }),
            DecodeDataFrameState::DataLen => self.consume_u32_le()
                .map(|dl| if dl > 0 {
                    DecodeDataFrameState::Data(dl).into()
                } else {
                    DecoderState::PayloadEndChar(PayloadType::DataFrame, b'S')
                }),
            DecodeDataFrameState::Data(len) => self.consume_bytes(len as usize).map(|data| {
                self.data_frame.data = data;
                DecoderState::PayloadEndChar(PayloadType::DataFrame, b'S')
            })
        }
    }

    fn decode_list_frames(&mut self, inner: DecodeListFramesState) -> Option<DecoderState> {
        match inner {
            DecodeListFramesState::NumFrames => self.consume_u32_le()
                .map(|nf| {
                    self.list_frames.num_frames = nf;
                    if self.list_frames.num_frames > 0 {
                        DecodeListFramesState::FrameId.into()
                    } else {
                        DecoderState::PayloadEndChar(PayloadType::ListFrames, b'L')
                    }
                }),
            DecodeListFramesState::FrameId => self.consume_u32_le()
                .map(|fid| {
                    self.list_frames.frame_id = fid;
                    DecodeListFramesState::FrameNameLen.into()
                }),
            DecodeListFramesState::FrameNameLen => self.consume_u8()
                .map(|fl| DecodeListFramesState::FrameName(fl).into()),
            DecodeListFramesState::FrameName(len) => self.consume_string(len as usize).map(|fname| {
                self.list_frames.frames.push(FrameInfo {
                    id: self.list_frames.frame_id,
                    name: fname,
                });

                if self.list_frames.frames.len() == (self.list_frames.num_frames as usize) {
                    DecoderState::PayloadEndChar(PayloadType::ListFrames, b'L')
                } else {
                    DecodeListFramesState::FrameId.into()
                }
            }),
        }
    }

    fn decode_get_frame_info(&mut self, inner: DecodeGetFrameInfoState) -> Result<Option<DecoderState>, String> {
        match inner {
            DecodeGetFrameInfoState::IsEnabled => self.consume_u8()
                .map(|ie| match ie {
                    0x00 => {
                        self.get_frame_info.enabled = false;
                        Ok(DecodeGetFrameInfoState::NumSignals.into())
                    }
                    0x01 => {
                        self.get_frame_info.enabled = true;
                        Ok(DecodeGetFrameInfoState::NumSignals.into())
                    }
                    _ => Err(format!("Invalid frame enabled value {ie}"))
                }).transpose(),
            DecodeGetFrameInfoState::NumSignals => Ok(self.consume_u32_le()
                .map(|ns| {
                    self.get_frame_info.num_signals = ns;
                    if self.get_frame_info.num_signals > 0 {
                        DecodeGetFrameInfoState::SignalNameLen.into()
                    } else {
                        DecoderState::PayloadEndChar(PayloadType::GetFrameInfo, b'I')
                    }
                })),
            DecodeGetFrameInfoState::SignalNameLen =>
                Ok(self.consume_u8()
                    .map(|snl| DecodeGetFrameInfoState::SignalName(snl).into())),
            DecodeGetFrameInfoState::SignalName(len) =>
                Ok(self.consume_string(len as usize).map(|sname| {
                    self.get_frame_info.signal_name = sname;
                    DecodeGetFrameInfoState::SignalTypeLen.into()
                })),
            DecodeGetFrameInfoState::SignalTypeLen =>
                Ok(self.consume_u8()
                    .map(|stl| DecodeGetFrameInfoState::SignalType(stl).into())),
            DecodeGetFrameInfoState::SignalType(len) =>
                Ok(self.consume_string(len as usize)
                    .and_then(|tyname| parse_type_name(&tyname))
                    .map(|ty| {
                        self.get_frame_info.signals.push(SignalInfo {
                            name: self.get_frame_info.signal_name.clone(),
                            ty,
                        });

                        if self.get_frame_info.signals.len() == (self.get_frame_info.num_signals as usize) {
                            DecoderState::PayloadEndChar(PayloadType::GetFrameInfo, b'I')
                        } else {
                            DecodeGetFrameInfoState::SignalNameLen.into()
                        }
                    }))
        }
    }


    fn consume_u8(&mut self) -> Option<u8> {
        if self.unread_bytes_count() < 1 {
            None
        } else {
            let ret = self.buffer[self.offset];
            self.offset += 1;
            Some(ret)
        }
    }

    fn consume_u16_le(&mut self) -> Option<u16> {
        if self.unread_bytes_count() < 2 {
            None
        } else {
            let u32_bytes: [u8; 2] = self.buffer.as_slices().0[self.offset..self.offset + 2].try_into().unwrap();
            let ret = u16::from_le_bytes(u32_bytes);
            self.offset += 2;
            Some(ret)
        }
    }

    fn consume_u32_le(&mut self) -> Option<u32> {
        if self.unread_bytes_count() < 4 {
            None
        } else {
            let u32_bytes: [u8; 4] = self.buffer.as_slices().0[self.offset..self.offset + 4].try_into().unwrap();
            let ret = u32::from_le_bytes(u32_bytes);
            self.offset += 4;
            Some(ret)
        }
    }

    fn peek_u32_le(&self) -> Option<u32> {
        if self.unread_bytes_count() < 4 {
            None
        } else {
            let u32_bytes: [u8; 4] = self.buffer.as_slices().0[self.offset..self.offset + 4].try_into().unwrap();
            Some(u32::from_le_bytes(u32_bytes))
        }
    }

    fn consume_string(&mut self, len: usize) -> Option<String> {
        if self.unread_bytes_count() < len {
            None
        } else {
            let ret = String::from(core::str::from_utf8(&self.buffer.as_slices().0[self.offset..self.offset + len]).unwrap());
            self.offset += len;
            Some(ret)
        }
    }

    fn consume_bytes(&mut self, len: usize) -> Option<Vec<u8>> {
        if self.unread_bytes_count() < len {
            None
        } else {
            let ret = self.buffer.as_slices().0[self.offset..self.offset + len].to_vec();
            self.offset += len;
            Some(ret)
        }
    }

    fn clear_read(&mut self) {
        self.buffer.drain(..self.offset);
        self.offset = 0;
    }

    fn unread_bytes_count(&self) -> usize {
        self.buffer.len() - self.offset
    }
}

impl DecodeResult {
    pub fn is_some(&self) -> bool {
        !matches!(self, DecodeResult::None)
    }
}