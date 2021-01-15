use std::{ffi::c_void, usize};

use iracingsdk::ptr::{FromPtr, Ptr, TryFromPtr};
use winapi::um::{
    errhandlingapi::GetLastError,
    handleapi::CloseHandle,
    memoryapi::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_COPY, FILE_MAP_READ},
};

pub const TELEMETRY_PATH: &str = r"Local\IRSDKMemMapFileName";
pub const DATA_EVENT_NAME: &str = r"Local\IRSDKDataValidEvent";

pub const MAX_BUFS: usize = 4;
pub const MAX_STRING: usize = 32;
pub const MAX_DESC: usize = 64;

#[derive(Debug)]
enum Error {
    TelemetryNotPresent,
    UnknownVarType(i32),
}

enum VarType {
    Char,
    Bool,
    Int,
    BitField,
    Float,
    Double,
}

impl VarType {
    fn bytes(&self) -> usize {
        match self {
            VarType::Char | VarType::Bool => 1,
            VarType::Int | VarType::BitField | VarType::Float => 4,
            VarType::Double => 8,
        }
    }
}

impl TryFromPtr for VarType {
    type Err = Error;

    unsafe fn try_from_ptr(ptr: &mut Ptr) -> Result<Self, Self::Err> {
        let int = ptr.read_i32();
        let var_type = match int {
            0 => VarType::Char,
            1 => VarType::Bool,
            2 => VarType::Int,
            3 => VarType::BitField,
            4 => VarType::Float,
            5 => VarType::Double,
            x => return Err(Error::UnknownVarType(x)),
        };
        Ok(var_type)
    }
}

struct VarHeader {
    r#type: VarType,
    offset: i32,
    count: i32,
    count_as_time: bool,
}

impl VarHeader {
    pub fn byte_len(&self) -> usize {
        self.r#type.bytes() * self.count as usize
    }
}

impl TryFromPtr for VarHeader {
    type Err = Error;

    unsafe fn try_from_ptr(ptr: &mut Ptr) -> Result<Self, Self::Err> {
        ptr.scoped(|p| {
            let r#type = VarType::try_from_ptr(p)?;
            let offset = p.read_i32();
            let count = p.read_i32();
            let count_as_time = p.read_bool();
            p.advance(3 + MAX_DESC + 2 * MAX_STRING);
            Ok(Self {
                r#type,
                offset,
                count,
                count_as_time,
            })
        })
    }
}

#[derive(Debug)]
struct VarBuf {
    tick_count: i32,
    buf_offset: i32,
}

impl FromPtr for VarBuf {
    unsafe fn from_ptr(ptr: &mut Ptr) -> Self {
        let out = Self {
            tick_count: ptr.read_i32(),
            buf_offset: ptr.read_i32(),
        };
        ptr.advance(16);
        out
    }
}

#[derive(Debug)]
struct SdkHeader {
    version: i32,
    status: i32,
    tick_rate: i32,
    session_info_update: i32,
    session_info_len: i32,
    session_info_offset: i32,
    num_vars: i32,
    var_header_offset: i32,
    num_buf: i32,
    buf_len: i32,
    var_bufs: [VarBuf; MAX_BUFS],
}

impl FromPtr for SdkHeader {
    unsafe fn from_ptr(ptr: &mut Ptr) -> Self {
        Self {
            version: ptr.read_i32(),
            status: ptr.read_i32(),
            tick_rate: ptr.read_i32(),
            session_info_update: ptr.read_i32(),
            session_info_len: ptr.read_i32(),
            session_info_offset: ptr.read_i32(),
            num_vars: ptr.read_i32(),
            var_header_offset: ptr.read_i32(),
            num_buf: ptr.read_i32(),
            buf_len: ptr.read_i32(),
            var_bufs: {
                ptr.advance(16);
                [
                    VarBuf::from_ptr(ptr),
                    VarBuf::from_ptr(ptr),
                    VarBuf::from_ptr(ptr),
                    VarBuf::from_ptr(ptr),
                ]
            },
        }
    }
}

fn main() -> Result<(), Error> {
    let telemetry_path = TELEMETRY_PATH
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();

    let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, false.into(), telemetry_path.as_ptr()) };
    if mapping.is_null() {
        let errno = unsafe { GetLastError() };
        println!("OpenFileMappingW error: {:X}", errno);
        return Err(Error::TelemetryNotPresent);
    }

    let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_COPY, 0, 0, 0) };
    if mapping.is_null() {
        let errno = unsafe { GetLastError() };
        println!("MapViewOfFile error: {:X}", errno);
        return Err(Error::TelemetryNotPresent);
    }

    let mut ptr = Ptr::new(view as *const c_void);
    let varbuf_ptr = ptr.clone();

    let header = unsafe { SdkHeader::from_ptr(&mut ptr) };

    println!("{:#?}", header);

    unsafe {
        UnmapViewOfFile(view);
        CloseHandle(mapping);
    };

    Ok(())
}
