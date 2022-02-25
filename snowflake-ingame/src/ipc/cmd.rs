use std::fmt::{Debug, Formatter};
use std::io::ErrorKind;
use uuid::Uuid;

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
    pub source_pid: i32,
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

impl Debug for GameWindowCommand {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("GameWindowCommand {{ ty: {} }}", self.ty.0))
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct Size {
    width: u32,
    height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Size {
        Size { width, height }
    }
}

impl GameWindowCommand {
    pub const fn handshake(uuid: &Uuid) -> GameWindowCommand {
        GameWindowCommand {
            magic: GameWindowMagic::MAGIC,
            ty: GameWindowCommandType::HANDSHAKE,
            params: GameWindowCommandParams {
                handshake_event: HandshakeEventParams {
                    uuid: *uuid
                }
            }
        }
    }

    pub const fn window_resize(size: &Size) -> GameWindowCommand {
        GameWindowCommand {
            magic: GameWindowMagic::MAGIC,
            ty: GameWindowCommandType::WINDOW_RESIZE,
            params: GameWindowCommandParams {
                resize_event: WindowResizeEventParams {
                    height: size.height as i32,
                    width: size.width as i32
                }
            }
        }
    }
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

impl <'a> Into<Vec<u8>> for GameWindowCommand {
    fn into(self) -> Vec<u8> {
        unsafe {
            Vec::from(any_as_u8_slice(&self))
        }
    }
}

impl <'a> TryFrom<&'a [u8]> for &'a GameWindowCommand {
    type Error = std::io::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let (head, body, _tail) = unsafe { value.align_to::<GameWindowCommand>() };
        if !head.is_empty() {
            return Err(std::io::Error::new(ErrorKind::InvalidData, "Received data was not aligned."))
        }
        let cmd_struct = &body[0];
        if !cmd_struct.magic.is_valid() {
            return Err(std::io::Error::new(ErrorKind::InvalidData, "Unexpected magic number for command packet."))
        }
        Ok(cmd_struct)
    }
}

impl <'a> TryFrom<&'a mut [u8]> for GameWindowCommand {
    type Error = std::io::Error;

    fn try_from(value: &'a mut [u8]) -> Result<Self, Self::Error> {
        let cmd_struct: &GameWindowCommand = &value.try_into()?;
        Ok(*cmd_struct)
    }
}

impl <'a> TryFrom<&'a [u8]> for GameWindowCommand {
    type Error = std::io::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let cmd_struct: &GameWindowCommand = value.try_into()?;
        Ok(*cmd_struct)
    }
}