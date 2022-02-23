pub mod cmd;

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::ErrorKind;
use std::mem;
use std::time::Duration;

use tokio::{io, time};
use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use windows::Win32::Foundation::ERROR_PIPE_BUSY;
use crate::ipc::cmd::{GameWindowCommand, GameWindowCommandType};
use crate::ipc::IpcConnectError::InvalidHandshake;

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

pub struct IpcConnection {
    ctx : Runtime,
    pipe: Option<NamedPipeClient>,
    cmd_client_tx: Option<tokio::sync::mpsc::UnboundedSender<GameWindowCommand>>,
    cmd_client_rx: Option<crossbeam_channel::Receiver<GameWindowCommand>>,
    cmd_rx: Option<tokio::sync::mpsc::UnboundedReceiver<GameWindowCommand>>,
    cmd_tx: Option<crossbeam_channel::Sender<GameWindowCommand>>,
    uuid: Uuid,
}

#[derive(Clone)]
pub struct IpcHandle {
    sender: tokio::sync::mpsc::UnboundedSender<GameWindowCommand>,
    events: crossbeam_channel::Receiver<GameWindowCommand>,
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

            let handshake = GameWindowCommand::handshake(pipeid);

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

        let (client_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (tx, client_rx) = crossbeam_channel::unbounded();
        self.cmd_rx = Some(rx);
        self.cmd_tx = Some(tx);
        self.cmd_client_tx = Some(client_tx);
        self.cmd_client_rx = Some(client_rx);
        Ok(())
    }

    pub fn new(uuid: Uuid) -> IpcConnection {
       IpcConnection { ctx: Runtime::new().unwrap(), uuid,
           pipe: None,
           cmd_client_tx: None,
           cmd_client_rx: None,
           cmd_rx: None,
           cmd_tx: None
       }
    }

    pub fn handle(&self) -> Option<IpcHandle> {
        if let (Some(snd), Some(recv))
            = (&self.cmd_client_tx, &self.cmd_client_rx) {
            return Some(IpcHandle {
                sender: UnboundedSender::clone(&snd),
                events: crossbeam_channel::Receiver::clone(&recv)
            });
        }
        None
    }

    pub fn listen(mut self) -> Result<(), Box<dyn Error>> {
        let mut client = self.pipe.unwrap();
        let mut cmd_rx = self.cmd_rx.unwrap();
        let mut cmd_tx = self.cmd_tx.unwrap();
        self.ctx.block_on(async move {
            loop {
                let ready = client.ready(Interest::READABLE | Interest::WRITABLE).await?;

                if ready.is_readable() {
                    let mut data = vec![0u8; mem::size_of::<GameWindowCommand>()];
                    match client.try_read(&mut data) {
                        Ok(_n) => {
                            match TryInto::<&GameWindowCommand>::try_into(data.as_slice()) {
                                Ok(cmd) => {
                                    match cmd_tx.send(*cmd) {
                                        Ok(()) => {
                                          //  println!("[ipc] Recv cmd {}", cmd.ty.0)
                                        },
                                        Err(e) => println!("[ipc] bcast error {:?}", e)
                                    }
                                }
                                Err(e) => {
                                    println!("[ipc] Invalid recv {:?}", e);
                                }
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            continue;
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                }

                if ready.is_writable() {
                    match cmd_rx.try_recv() {
                        Ok(cmd) =>  {
                            match client.try_write((&cmd).into()) {
                                Ok(_n) => {
                                    // println!("[ipc] Send cmd {}", cmd.ty.0);
                                }
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    continue;
                                }
                                Err(e) => {
                                    return Err(e.into());
                                }
                            }
                        }
                        Err(_e) => {}
                    }
                }
            }
            Ok::<_, io::Error>(())
        }).unwrap();
        eprintln!("loop done");
        Ok(())
    }
}

impl IpcHandle {
    pub fn send(&self, cmd: GameWindowCommand) -> Result<(), tokio::sync::mpsc::error::SendError<GameWindowCommand>> {
        self.sender.send(cmd)
    }

    pub fn recv(&self) -> Result<GameWindowCommand, crossbeam_channel::RecvError> {
        self.events.recv()
    }

    pub fn try_recv(&self) -> Result<GameWindowCommand, crossbeam_channel::TryRecvError> {
        self.events.try_recv()
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