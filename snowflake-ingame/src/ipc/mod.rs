pub mod cmd;

use std::error::Error;
use std::fmt::{Display, Formatter};
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
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
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

pub struct IpcConnection {
    ctx: Runtime,
    pipe: NamedPipeClient,
    remote_tx: tokio::sync::mpsc::UnboundedSender<GameWindowCommand>,
    local_rx: crossbeam_channel::Receiver<GameWindowCommand>,
    remote_rx: tokio::sync::mpsc::UnboundedReceiver<GameWindowCommand>,
    local_tx: crossbeam_channel::Sender<GameWindowCommand>,
    kill_rx: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl IpcConnectionBuilder {
    pub fn connect(
        self,
        mut kill_rx: Option<tokio::sync::oneshot::Receiver<()>>,
    ) -> Result<IpcConnection, Box<dyn Error>> {
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
            remote_rx: rx,
            local_tx: tx,
            remote_tx: client_tx,
            local_rx: client_rx,
            kill_rx,
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
            sender: UnboundedSender::clone(&self.remote_tx),
            events: crossbeam_channel::Receiver::clone(&self.local_rx),
        }
    }

    pub fn listen(self) -> Result<(), Box<dyn Error>> {
        let client = self.pipe;
        let mut remote_rx = self.remote_rx;
        let local_tx = self.local_tx;

        // Prevent this from clogging up the channel.
        drop(self.local_rx);

        // No more handles can be created.
        drop(self.remote_tx);

        let mut kill_rx = self.kill_rx;

        self.ctx
            .block_on(async move {
                loop {
                    if let Some(Ok(())) = kill_rx.as_mut().map(|r| r.try_recv()) {
                        println!("[ipc] kill signal received");
                        break Ok(());
                    }

                    let ready = client
                        .ready(Interest::READABLE | Interest::WRITABLE)
                        .await?;

                    if ready.is_readable() {
                        let mut data = vec![0u8; mem::size_of::<GameWindowCommand>()];
                        match client.try_read(&mut data) {
                            Ok(_n) => {
                                match TryInto::<&GameWindowCommand>::try_into(data.as_slice()) {
                                    Ok(cmd) => {
                                        match local_tx.send(*cmd) {
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
                                return Err::<(), io::Error>(e.into());
                            }
                        }
                    }

                    if ready.is_writable() {
                        match remote_rx.try_recv() {
                            Ok(cmd) => {
                                match client.try_write((&cmd).into()) {
                                    Ok(_n) => {
                                        // println!("[ipc] Send cmd {}", cmd.ty.0);
                                    }
                                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                        continue;
                                    }
                                    Err(e) => {
                                        return Err::<(), io::Error>(e.into());
                                    }
                                }
                            }
                            Err(_e) => {}
                        }
                    }
                }
            })
            .unwrap();
        eprintln!("[ipc] listen loop complete");
        Ok(())
    }
}

impl IpcHandle {
    pub fn send(
        &self,
        cmd: GameWindowCommand,
    ) -> Result<(), Box<tokio::sync::mpsc::error::SendError<GameWindowCommand>>> {
        self.sender.send(cmd).map_err(|e| Box::new(e))
    }

    #[allow(dead_code)]
    pub fn recv(&self) -> Result<GameWindowCommand, crossbeam_channel::RecvError> {
        self.events.recv()
    }

    pub fn try_recv(&self) -> Result<GameWindowCommand, crossbeam_channel::TryRecvError> {
        self.events.try_recv()
    }
}
