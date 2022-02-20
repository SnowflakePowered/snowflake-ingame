use std::error::Error;
use std::time::Duration;
use ipipe::{OnCleanup, Pipe};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::time;
use uuid::Uuid;
use windows::Win32::Foundation::ERROR_PIPE_BUSY;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[repr(transparent)]
pub struct GameWindowCommandType(u8);

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct MouseButton(u8);

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct ModifierKey(u8);

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct Cursor(u8);

impl GameWindowCommandType {
    pub const HANDSHAKE: GameWindowCommandType = Self(1);
    pub const WINDOW_RESIZE: GameWindowCommandType = Self(2);
    pub const WINDOW_MESSAGE: GameWindowCommandType = Self(3);
    pub const MOUSE: GameWindowCommandType = Self(4);
    pub const CURSOR: GameWindowCommandType = Self(5);
    pub const OVERLAY: GameWindowCommandType = Self(6);
    pub const SHUTDOWN: GameWindowCommandType = Self(7);
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
#[repr(transparent)]
pub struct GameWindowMagic(u8);
impl GameWindowMagic {
    pub fn is_valid(self) -> bool {
        self == GameWindowMagic::MAGIC
    }

    pub const MAGIC: GameWindowMagic = Self(0x9f);
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HandshakeEventParams {
    pub uuid: Uuid
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CursorEventParams {
    pub cursor: Cursor
}


#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct OverlayTextureEventParams {
    pub handle: usize,
    pub source_pid: usize,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub alignment: u64,
    pub sync_handle: usize
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct WindowMessageEventParams {
    pub msg: i32,
    pub wparam: u64,
    pub lparam: i32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct WindowResizeEventParams {
    pub height: i32,
    pub width: i32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MouseEventParams {
    pub mouse_double_click: MouseButton,
    pub mouse_down: MouseButton,
    pub mouse_up: MouseButton,
    pub modifiers: ModifierKey,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub wheel_x: f32,
    pub wheel_y: f32
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub union GameWindowCommandParams {
    pub handshake_event: HandshakeEventParams,
    pub resize_event: WindowResizeEventParams,
    pub window_message_event: WindowMessageEventParams,
    pub mouse_event: MouseEventParams,
    pub cursor_event: CursorEventParams,
    pub overlay_event: OverlayTextureEventParams,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct GameWindowCommand {
    pub magic: GameWindowMagic,
    pub ty: GameWindowCommandType,
    pub params: GameWindowCommandParams
}

pub async fn connect(pipeid: Uuid) -> Result<NamedPipeClient, Box<dyn Error>> {
    let pipe_name = format!(r"\\.\pipe\Snowflake.Orchestration.Renderer-{}", pipeid.to_simple());
    let client = loop {
        match ClientOptions::new().open(&pipe_name) {
            Ok(client) => break client,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => (),
            Err(e) => return Err(e.into()),
        }

        time::sleep(Duration::from_millis(50)).await;
    };

    Ok(client)
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

impl <'a> Into<&'a [u8]> for &'a GameWindowCommand {
    fn into(self) -> &'a [u8] {
        unsafe {
            any_as_u8_slice(self)
        }
    }
}