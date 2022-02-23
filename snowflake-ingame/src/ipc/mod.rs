use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::{ErrorKind, Read, Write};
use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use ipipe::{OnCleanup, Pipe};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::{io, time};
use tokio::runtime::{Handle, Runtime};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use uuid::Uuid;
use windows::Win32::Foundation::ERROR_PIPE_BUSY;
use crate::ipc::IpcConnectError::InvalidHandshake;

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

#[derive(Debug)]
pub enum IpcConnectError {
    InvalidHandshake
}

impl Display for IpcConnectError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcConnectError::InvalidHandshake => "Invalid handshake"
        };
        Ok(())
    }
}

impl std::error::Error for IpcConnectError {}

pub async fn connect(pipeid: &Uuid) -> Result<NamedPipeClient, Box<dyn Error>> {
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

pub fn connect_ipipe(pipeid: Uuid) -> Result<Pipe, Box<dyn Error>> {
    let pipe_name = format!(r"\\.\pipe\Snowflake.Orchestration.Renderer-{}", pipeid.to_simple());
    let client = Pipe::open(pipe_name.as_ref(), OnCleanup::NoDelete)?;
    Ok(client)
}

pub struct IpcConnection {
    ctx : Runtime,
    pipe: Option<NamedPipeClient>,
    cmd_sender: Option<tokio::sync::mpsc::UnboundedSender<GameWindowCommand>>,
    cmd_rx: Option<tokio::sync::mpsc::UnboundedReceiver<GameWindowCommand>>,

    // cmd_in_src: tokio::sync::broadcast::Sender<GameWindowCommand>,
    // listen_thread: JoinHandle<()>
   uuid: Uuid,
}

pub struct IpcHandle {
    ctx: Handle,
    pipe: Arc<tokio::sync::RwLock<NamedPipeClient>>
}

async fn try_read_pipe(pipe: &NamedPipeClient, buf: &mut [u8]) -> Result<Option<GameWindowCommand>, Box<dyn Error + Send + Sync>> {
    pipe.readable().await?;
    match pipe.try_read(buf) {
        Ok(0) => return Err(io::Error::new(io::ErrorKind::BrokenPipe, "Pipe closed when expected handshake").into()),
        Ok(n) => Ok(Some(buf.try_into()?)),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => return Err(e.into())
    }
}

impl IpcConnection {
    pub fn connect(&mut self, pipeid: Uuid) -> Result<(), Box<dyn Error>> {
        let pipe = self.ctx.block_on(async {
            let mut pipe = connect(&pipeid).await?;

            let handshake = GameWindowCommand {
                magic: GameWindowMagic::MAGIC,
                ty: GameWindowCommandType::HANDSHAKE,
                params: GameWindowCommandParams {
                    handshake_event: HandshakeEventParams {
                        uuid: Uuid::nil()
                    }
                }
            };

            let handshake_bytes: Vec<u8> = handshake.into();
            pipe.write(&handshake_bytes).await?;

            let mut handshake_recv = vec![0u8; std::mem::size_of::<GameWindowCommand>()];
            pipe.read_exact(&mut handshake_recv).await?;

            let handshake: GameWindowCommand = handshake_recv.as_slice().try_into()?;

            if handshake.ty != GameWindowCommandType::HANDSHAKE {
                return Err(InvalidHandshake.into());
            }

            if unsafe { handshake.params.handshake_event.uuid } != pipeid {
                return Err(InvalidHandshake.into());
            }

            Ok::<_, Box<dyn Error>>(pipe)
        })?;

        self.pipe = Some(pipe);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.cmd_rx = Some(rx);
        self.cmd_sender = Some(tx);
        Ok(())
    }

    fn connect_(&self) -> Result<NamedPipeClient, Box<dyn Error>> {
        let pipe = self.ctx.block_on(async {
            let mut pipe = connect(&self.uuid).await?;

            let handshake = GameWindowCommand {
                magic: GameWindowMagic::MAGIC,
                ty: GameWindowCommandType::HANDSHAKE,
                params: GameWindowCommandParams {
                    handshake_event: HandshakeEventParams {
                        uuid: Uuid::nil()
                    }
                }
            };

            let handshake_bytes: Vec<u8> = handshake.into();
            pipe.write(&handshake_bytes).await?;

            let mut handshake_recv = vec![0u8; std::mem::size_of::<GameWindowCommand>()];
            pipe.read_exact(&mut handshake_recv).await?;

            let handshake: GameWindowCommand = handshake_recv.as_slice().try_into()?;

            if handshake.ty != GameWindowCommandType::HANDSHAKE {
                return Err(InvalidHandshake.into());
            }

            if unsafe { handshake.params.handshake_event.uuid } != self.uuid {
                return Err(InvalidHandshake.into());
            }

            Ok::<_, Box<dyn Error>>(pipe)
        })?;
        Ok(pipe)
    }

    pub fn new(uuid: Uuid) -> IpcConnection {
       IpcConnection { ctx: Runtime::new().unwrap(), uuid, pipe: None, cmd_sender: None, cmd_rx: None }
    }

    pub fn handle(&self) -> Option<tokio::sync::mpsc::UnboundedSender<GameWindowCommand>> {
        if let Some(snd) = &self.cmd_sender {
            return Some(snd.clone());
        }
        None
    }

    pub fn listen(mut self) -> Result<(), Box<dyn Error>> {
        let mut pipe = tokio::sync::Mutex::new(self.pipe.unwrap());
        let mut cmd_rx = self.cmd_rx.unwrap();
        self.ctx.block_on(async move {
            loop {
                let mut m = Vec::new();
                tokio::select! {
                    Ok(Some(cmd)) = async {
                        try_read_pipe(&pipe.lock().await.deref(), &mut m).await
                    }
                    => {
                        eprintln!("r {}", cmd.ty.0);
                    }
                    res = async { if let Some(cmd) = cmd_rx.recv().await {
                        &pipe.lock().await.write_all((&cmd).into()).await?;
                    }  Ok::<_, io::Error>(()) } => {
                        match res {
                            Ok(_) =>   eprintln!("s"),
                            Err(e) => eprintln!("e {:?}", e)
                        }

                    }
                    else => { }
                }
            }
        });

        Ok(())
    }
    // pub fn make_sender(&self) -> tokio::sync::mpsc::Sender<GameWindowCommand> {
    //     self.cmd_sender.clone()
    // }

    // pub fn make_receiver(&self) -> tokio::sync::broadcast::Receiver<GameWindowCommand> {
    //     self.cmd_in_src.subscribe()
    // }

    // pub async fn listen(self) {
    //     self.listen_thread.await;
    // }
    // fn send(&self, cmd: GameWindowCommand) -> JoinHandle<bool> {
    //     self.rt.spawn(async move {
    //         let cmd = cmd;
    //         let cmd: &[u8] = (&cmd).into();
    //         loop {
    //             if self.pipe.writable().await.is_err() {
    //                 break false
    //             }
    //             match self.pipe.try_write(cmd) {
    //                 Ok(0) => break false,
    //                 Ok(_n) => break true,
    //                 Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
    //                 Err(_e) => break false
    //             }
    //         }
    //     })
    // }
}

impl IpcHandle {
    pub fn send(&self, cmd: GameWindowCommand) {
        let pipe = Arc::clone(&self.pipe);
        self.ctx.spawn(async move {
            // let mut pipe = pipe.write().await;
            // pipe.writable().await?;
            // pipe.write_all((&cmd).into()).await?;
            match pipe.try_write() {
                Ok(mut pipe) => {
                    loop {
                        eprintln!("sending {}", cmd.ty.0);
                        pipe.writable().await?;
                        match pipe.try_write((&cmd).into()) {
                            Ok(b) => break,
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                            Err(e) => {
                                eprintln!("{:?}", e);
                            }
                        }
                    }
                    eprintln!("dropped lock");
                    drop(pipe);
                }
                Err(e) => {
                    eprintln!("{:?}", e);
                }
            }

            Ok::<_, io::Error>(())
        });
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
    type Error = io::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let (head, body, _tail) = unsafe { value.align_to::<GameWindowCommand>() };
        if !head.is_empty() {
            return Err(io::Error::new(ErrorKind::InvalidData, "Received data was not aligned."))
        }
        let cmd_struct = &body[0];
        if !cmd_struct.magic.is_valid() {
            return Err(io::Error::new(ErrorKind::InvalidData, "Unexpected magic number for command packet."))
        }
        Ok(cmd_struct)
    }
}

impl <'a> TryFrom<&'a mut [u8]> for GameWindowCommand {
    type Error = io::Error;

    fn try_from(value: &'a mut [u8]) -> Result<Self, Self::Error> {
        let cmd_struct: &GameWindowCommand = &value.try_into()?;
        Ok(*cmd_struct)
    }
}

impl <'a> TryFrom<&'a [u8]> for GameWindowCommand {
    type Error = io::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let cmd_struct: &GameWindowCommand = value.try_into()?;
        Ok(*cmd_struct)
    }
}