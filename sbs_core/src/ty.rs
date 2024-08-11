use std::str::FromStr;
use regex::Regex;

#[derive(Clone, Debug)]
pub enum Type {
    Uint8,
    Uint16,
    Uint32,
    Int8,
    Int16,
    Int32,
    Float32,
    SFix(u32, i32),
    UFix(u32, i32),
}

pub fn parse_type_name(ty_name: &str) -> Option<Type> {
    let re = Regex::new(r"^(?<named>(uint8)|(uint16)|(uint32)|(int8)|(int16)|(int32)|(float32))|((?<fix_base>(ufix)|(sfix))\((?<fix_wlen>[0-9]+),( )*(?<fix_exp>-?[0-9]+)\))$").unwrap();

    match re.captures(ty_name) {
        Some(caps) => {
            if caps.name("named").is_some() {
                match caps.name("named").unwrap().as_str() {
                    "uint8" => Some(Type::Uint8),
                    "uint16" => Some(Type::Uint16),
                    "uint32" => Some(Type::Uint32),
                    "int8" => Some(Type::Int8),
                    "int16" => Some(Type::Int16),
                    "int32" => Some(Type::Int32),
                    "float32" => Some(Type::Float32),
                    _ => None
                }
            } else if caps.name("fix_base").is_some() {
                let wlen = u32::from_str(caps.name("fix_wlen").unwrap().as_str()).ok()?;
                let exp = i32::from_str(caps.name("fix_exp").unwrap().as_str()).ok()?;

                match caps.name("fix_base").unwrap().as_str() {
                    "sfix" => Some(Type::SFix(wlen, exp)),
                    "ufix" => Some(Type::UFix(wlen, exp)),
                    _ => None
                }
            } else {
                None
            }
        }
        None => None
    }
}
