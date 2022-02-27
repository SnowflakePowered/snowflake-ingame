pub mod cmd;

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::ErrorKind;
use std::mem;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, Interest};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::UnboundedSender;
use tokio::{io, time};
use uuid::Uuid;

use crate::ipc::cmd::{GameWindowCommand, GameWindowCommandType};
use crate::ipc::IpcConnectError::InvalidHandshake;
use windows::Win32::Foundation::ERROR_PIPE_BUSY;

#[derive(Debug)]
pub enum IpcConnectError {
    InvalidHandshake,
}

impl Display for IpcConnectError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcConnectError::InvalidHandshake => "Invalid handshake",
        };
        Ok(())
    }
}

impl std::error::Error for IpcConnectError {}

pub async fn connect(pipeid: &Uuid) -> Result<NamedPipeClient, Box<dyn Error>> {
    let pipe_name = format!(
        r"\\.\pipe\Snowflake.Orchestration.Renderer-{}",
        pipeid.to_simple()
    );
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

pub struct IpcConnectionBuilder {
    ctx: Runtime,
    uuid: Uuid,
}

#[derive(Clone)]
pub struct IpcHandle {
    sender: tokio::sync::mpsc::UnboundedSender<GameWindowCommand>,
    events: crossbeam_channel::Receiver<GameWindowCommand>,
}

async fn try_read_pipe(
    pipe: &NamedPipeClient,
    buf: &mut [u8],
) -> Result<Option<GameWindowCommand>, Box<dyn Error + Send + Sync>> {
    pipe.readable().await?;
    match pipe.try_read(buf) {
        Ok(0) => {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Pipe closed when expected handshake",
            )
            .into())
        }
        Ok(n) => Ok(Some(buf.try_into()?)),
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => return Err(e.into()),
    }
}

pub struct IpcConnection {
    ctx: Runtime,
    pipe: NamedPipeClient,
    cmd_client_tx: tokio::sync::mpsc::UnboundedSender<GameWindowCommand>,
    cmd_client_rx: crossbeam_channel::Receiver<GameWindowCommand>,
    cmd_rx: tokio::sync::mpsc::UnboundedReceiver<GameWindowCommand>,
    cmd_tx: crossbeam_channel::Sender<GameWindowCommand>,
    uuid: Uuid,
}

impl IpcConnectionBuilder {
    pub fn connect(mut self) -> Result<IpcConnection, Box<dyn Error>> {
        let pipe = self.ctx.block_on(async {
            let pipeid = &self.uuid;
            let mut pipe = connect(pipeid).await?;

            let handshake = GameWindowCommand::handshake(pipeid);

            let handshake_bytes: Vec<u8> = handshake.into();
            pipe.write(&handshake_bytes).await?;

            let mut handshake_recv = vec![0u8; std::mem::size_of::<GameWindowCommand>()];
            pipe.read_exact(&mut handshake_recv).await?;

            let handshake: GameWindowCommand = handshake_recv.as_slice().try_into()?;

            if handshake.ty != GameWindowCommandType::HANDSHAKE {
                return Err(InvalidHandshake.into());
            }

            if unsafe { &handshake.params.handshake_event.uuid } != pipeid {
                return Err(InvalidHandshake.into());
            }

            Ok::<_, Box<dyn Error>>(pipe)
        })?;

        let (client_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (tx, client_rx) = crossbeam_channel::unbounded();

        Ok(IpcConnection {
            ctx: self.ctx,
            pipe,
            cmd_rx: rx,
            cmd_tx: tx,
            cmd_client_tx: client_tx,
            cmd_client_rx: client_rx,
            uuid: self.uuid,
        })
    }

    pub fn new(uuid: Uuid) -> Self {
        Self {
            ctx: Runtime::new().unwrap(),
            uuid,
        }
    }
}

impl IpcConnection {
    pub fn handle(&self) -> IpcHandle {
        IpcHandle {
            sender: UnboundedSender::clone(&self.cmd_client_tx),
            events: crossbeam_channel::Receiver::clone(&self.cmd_client_rx),
        }
    }

    pub fn listen(mut self) -> Result<(), Box<dyn Error>> {
        let mut client = self.pipe;
        let mut cmd_rx = self.cmd_rx;
        let mut cmd_tx = self.cmd_tx;
        self.ctx
            .block_on(async move {
                loop {
                    let ready = client
                        .ready(Interest::READABLE | Interest::WRITABLE)
                        .await?;

                    if ready.is_readable() {
                        let mut data = vec![0u8; mem::size_of::<GameWindowCommand>()];
                        match client.try_read(&mut data) {
                            Ok(_n) => {
                                match TryInto::<&GameWindowCommand>::try_into(data.as_slice()) {
                                    Ok(cmd) => {
                                        match cmd_tx.send(*cmd) {
                                            Ok(()) => {
                                                //  println!("[ipc] Recv cmd {}", cmd.ty.0)
                                            }
                                            Err(e) => println!("[ipc] bcast error {:?}", e),
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
                            Ok(cmd) => {
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
            })
            .unwrap();
        eprintln!("loop done");
        Ok(())
    }
}

impl IpcHandle {
    pub fn send(
        &self,
        cmd: GameWindowCommand,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<GameWindowCommand>> {
        self.sender.send(cmd)
    }

    pub fn recv(&self) -> Result<GameWindowCommand, crossbeam_channel::RecvError> {
        self.events.recv()
    }

    pub fn try_recv(&self) -> Result<GameWindowCommand, crossbeam_channel::TryRecvError> {
        self.events.try_recv()
    }
}
