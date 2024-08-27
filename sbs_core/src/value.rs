use std::fmt::{Display, Formatter};
use crate::decode::BinaryReader;
use crate::sbs::SignalFrameDescriptor;
use crate::ty::Type;

#[derive(Clone, Debug)]
pub enum Value {
    Uint8(u8),
    Uint16(u16),
    Uint32(u32),
    Int8(i8),
    Int16(i16),
    Int32(i32),
    Float32(f32),
    SFix { w: u32, e: i32, raw: i64 },
    UFix { w: u32, e: i32, raw: u64 },
}

impl Type {
    pub fn default_value(&self) -> Value {
        match self {
            Type::Uint8 => Value::Uint8(0),
            Type::Uint16 => Value::Uint16(0),
            Type::Uint32 => Value::Uint32(0),
            Type::Int8 => Value::Int8(0),
            Type::Int16 => Value::Int16(0),
            Type::Int32 => Value::Int32(0),
            Type::Float32 => Value::Float32(0.0),
            Type::SFix(w, e) => Value::SFix { w: *w, e: *e, raw: 0 },
            Type::UFix(w, e) => Value::UFix { w: *w, e: *e, raw: 0 },
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::UFix { e, raw, .. } => {
                let mut approx = *raw as f64;
                if *e < 0 {
                    approx /= (2 << (-*e - 1)) as f64;
                } else if *e > 0 {
                    approx *= (2 << (e - 1)) as f64;
                }

                write!(f, "{}", approx)
            }
            _ => todo!()
        }
    }
}

impl Into<f64> for Value {
    fn into(self) -> f64 {
        match self {
            Value::Uint8(v) => v as f64,
            Value::Uint16(v) => v as f64,
            Value::Uint32(v) => v as f64,
            Value::Int8(v) => v as f64,
            Value::Int16(v) => v as f64,
            Value::Int32(v) => v as f64,
            Value::Float32(v) => v as f64,
            Value::SFix { .. } => todo!(),
            Value::UFix { e, raw, .. } => {
                let mut approx = raw as f64;
                if e < 0 {
                    approx /= (2 << (-e - 1)) as f64
                } else if e > 0 {
                    approx *= (2 << (e - 1)) as f64
                }

                approx
            }
        }
    }
}


#[derive(Clone, Debug)]
pub struct SignalFrameValue {
    pub descriptor: SignalFrameDescriptor,
    pub timestamp: u32,
    pub data: Vec<Value>,
}

impl SignalFrameValue {
    pub fn new(descriptor: SignalFrameDescriptor) -> SignalFrameValue {
        let data = descriptor.signals.iter().map(|s| s.ty.default_value()).collect::<Vec<_>>();

        SignalFrameValue {
            descriptor,
            timestamp: 0,
            data,
        }
    }

    pub fn update_from_bytes(&mut self, timestamp: u32, bytes: &[u8]) -> bool {
        self.timestamp = timestamp;

        let mut reader = BinaryReader::new(bytes);

        for (i, signal) in self.descriptor.signals.iter().enumerate() {
            match signal.ty.decode_bytes(&mut reader) {
                Some(data) => self.data[i] = data,
                None => return false,
            }
        }

        true
    }
}

impl Display for SignalFrameValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let signal_values = self.descriptor.signals
            .iter()
            .enumerate()
            .map(|(i, descriptor)| {
                let value = &self.data[i];

                format!("{}={value}", descriptor.name)
            })
            .collect::<Vec<_>>()
            .join(", ");


        if signal_values.is_empty() {
            write!(f, "{}(t={})", self.descriptor.name, self.timestamp)
        } else {
            write!(f, "{}(t={}, {})", self.descriptor.name, self.timestamp, signal_values)
        }
    }
}
