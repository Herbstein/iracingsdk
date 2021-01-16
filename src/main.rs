use std::{ffi::c_void, usize};

use iracingsdk::{
    ptr::{FromPtr, Ptr, TryFromPtr},
    Error,
};
use winapi::{
    shared::minwindef::{FALSE, LPCVOID},
    um::{
        errhandlingapi::GetLastError,
        handleapi::CloseHandle,
        memoryapi::{
            MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_COPY, FILE_MAP_READ,
        },
        synchapi::{OpenEventW, WaitForSingleObject},
        winbase::{INFINITE, WAIT_FAILED},
        winnt::{HANDLE, SYNCHRONIZE},
    },
};

pub const TELEMETRY_PATH: &str = r"Local\IRSDKMemMapFileName";
pub const DATA_EVENT_NAME: &str = r"Local\IRSDKDataValidEvent";

pub const MAX_BUFS: usize = 4;
pub const MAX_STRING: usize = 32;
pub const MAX_DESC: usize = 64;

#[derive(Debug)]
enum VarType {
    Char,
    Bool,
    Int,
    BitField,
    Float,
    Double,
}

impl VarType {
    fn byte_len(&self) -> usize {
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
        let int = ptr.read();
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

#[derive(Debug)]
struct VarHeader {
    r#type: VarType,
    offset: i32,
    count: i32,
    count_as_time: bool,
    name: [u8; MAX_STRING],
    desc: [u8; MAX_DESC],
    unit: [u8; MAX_STRING],
}

impl VarHeader {
    pub fn byte_len(&self) -> usize {
        self.r#type.byte_len() * self.count as usize
    }
}

impl TryFromPtr for VarHeader {
    type Err = Error;

    unsafe fn try_from_ptr(ptr: &mut Ptr) -> Result<Self, Self::Err> {
        ptr.scoped(|p| {
            let r#type = p.try_read()?;
            let offset = p.read();
            let count = p.read();
            let count_as_time = p.read();
            p.advance(3);
            let name = p.read();
            let desc = p.read();
            let unit = p.read();
            Ok(Self {
                r#type,
                offset,
                count,
                count_as_time,
                name,
                desc,
                unit,
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
            tick_count: ptr.read(),
            buf_offset: ptr.read(),
        };
        ptr.advance(8);
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
            version: ptr.read(),
            status: ptr.read(),
            tick_rate: ptr.read(),
            session_info_update: ptr.read(),
            session_info_len: ptr.read(),
            session_info_offset: ptr.read(),
            num_vars: ptr.read(),
            var_header_offset: ptr.read(),
            num_buf: ptr.read(),
            buf_len: ptr.read(),
            var_bufs: {
                ptr.advance(8);
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

struct Connection {
    mapping: HANDLE,
    event: HANDLE,
}

impl Connection {
    fn open() -> Result<Self, Error> {
        let telemetry_path = TELEMETRY_PATH
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();

        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, FALSE, telemetry_path.as_ptr()) };
        if mapping.is_null() {
            return Err(Error::TelemetryNotPresent);
        }

        let event_name = DATA_EVENT_NAME
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();

        let event = unsafe { OpenEventW(SYNCHRONIZE, FALSE, event_name.as_ptr()) };
        if mapping.is_null() {
            return Err(Error::EventNotPresent);
        }

        Ok(Connection { mapping, event })
    }

    fn wait_for_event(&self) -> Result<(), Error> {
        let res = unsafe { WaitForSingleObject(self.event, INFINITE) };
        match res {
            WAIT_FAILED => {
                let errno = unsafe { GetLastError() };
                Err(Error::WaitFailed(errno))
            }
            _ => Ok(()),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.event);
            CloseHandle(self.mapping);
        };
    }
}

struct View {
    view: LPCVOID,
}

impl View {
    fn create(conn: &Connection) -> Result<Self, Error> {
        let view = unsafe { MapViewOfFile(conn.mapping, FILE_MAP_READ | FILE_MAP_COPY, 0, 0, 0) };
        if view.is_null() {
            return Err(Error::ViewCreationFailed);
        }

        Ok(Self { view })
    }
}

impl Drop for View {
    fn drop(&mut self) {
        unsafe { UnmapViewOfFile(self.view) };
    }
}

fn main() -> Result<(), Error> {
    let connection = Connection::open()?;

    while {
        connection.wait_for_event()?;
        true
    } {
        let view = View::create(&connection)?;
        let mut ptr = Ptr::new(view.view as *const c_void);

        let sdk_header = unsafe { SdkHeader::from_ptr(&mut ptr) };

        ptr.set(sdk_header.var_header_offset as usize);

        let mut headers = Vec::new();
        for _ in 0..sdk_header.num_vars {
            headers.push(unsafe { ptr.try_read::<VarHeader>() }?);
        }

        for header in headers.iter() {
            println!(
                "{:?}",
                header.name.iter().map(|&b| b as char).collect::<Vec<_>>()
            );
        }

        let newest_buf = sdk_header
            .var_bufs
            .iter()
            .max_by_key(|v| v.tick_count)
            .unwrap();
        ptr.set(newest_buf.buf_offset as usize);
    }

    Ok(())
}
