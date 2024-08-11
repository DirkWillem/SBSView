use crate::ty::Type;
use crate::value::Value;

pub struct BinaryReader<'s> {
    bytes: &'s [u8],
}

impl<'s> BinaryReader<'s> {
    pub fn new(bytes: &'s [u8]) -> BinaryReader<'s> {
        BinaryReader { bytes }
    }

    pub fn read(&mut self, n: usize) -> Option<&'s [u8]> {
        if self.bytes.len() >= n {
            let result = &self.bytes[..n];
            self.bytes = &self.bytes[n..];
            Some(result)
        } else {
            None
        }
    }
}

impl Type {
    pub fn decode_bytes(&self, reader: &mut BinaryReader) -> Option<Value> {
        match self {
            Type::Uint8 => reader.read(1)
                .map(|data| Value::Uint8(data[0])),
            Type::Uint16 => reader.read(2)
                .map(|data| Value::Uint16(u16::from_le_bytes(<[u8; 2]>::try_from(data).unwrap()))),
            Type::Uint32 => reader.read(4)
                .map(|data| Value::Uint32(u32::from_le_bytes(<[u8; 4]>::try_from(data).unwrap()))),
            Type::Int8 => reader.read(1)
                .map(|data| Value::Int8(i8::from_le_bytes(<[u8; 1]>::try_from(data).unwrap()))),
            Type::Int16 => reader.read(2)
                .map(|data| Value::Int16(i16::from_le_bytes(<[u8; 2]>::try_from(data).unwrap()))),
            Type::Int32 => reader.read(4)
                .map(|data| Value::Int32(i32::from_le_bytes(<[u8; 4]>::try_from(data).unwrap()))),
            Type::UFix(w, e) if *w <= 8 => reader.read(1)
                .map(|data| Value::UFix { w: *w, e: *e, raw: data[0] as u64 }),
            Type::UFix(w, e) if *w <= 16 => reader.read(2)
                .map(|data| Value::UFix {
                    w: *w,
                    e: *e,
                    raw: u16::from_le_bytes(<[u8; 2]>::try_from(data).unwrap()) as u64,
                }),
            Type::UFix(w, e) if *w <= 32 => reader.read(4)
                .map(|data| Value::UFix {
                    w: *w,
                    e: *e,
                    raw: u32::from_le_bytes(<[u8; 4]>::try_from(data).unwrap()) as u64,
                }),
            Type::UFix(w, e) if *w <= 64 => reader.read(8)
                .map(|data| Value::UFix {
                    w: *w,
                    e: *e,
                    raw: u64::from_le_bytes(<[u8; 8]>::try_from(data).unwrap()) as u64,
                }),
            _ => todo!()
        }
    }
}
