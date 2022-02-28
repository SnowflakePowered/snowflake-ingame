#![feature(once_cell)]

use std::error::Error;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::panic::catch_unwind;

use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use windows::Win32::Foundation::{BOOL, HINSTANCE, RECT};
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Buffer, ID3D11Device, D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT, D3D11_VIEWPORT,
    D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

use crate::d3d11::hook_d3d11::Direct3D11HookContext;
use crate::hook::*;
use crate::ipc::cmd::GameWindowCommand;
use crate::ipc::{IpcConnection, IpcConnectionBuilder};
use crate::opengl::hook_opengl::OpenGLHookContext;

mod d3d11;
mod hook;
mod ipc;
mod opengl;
mod win32;

unsafe fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Direct3D11HookContext::init()?;

    // ctx.new(
    //     Box::new(|this, sync, flags, mut next| {
    //         let mut context = None;
    //
    //         unsafe {
    //             let device = this.GetDevice::<ID3D11Device>().unwrap();
    //             device.GetImmediateContext(&mut context);
    //         }
    //
    //         let context = context.unwrap();
    //
    //         let mut num = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
    //         let mut rects: [RECT;
    //             D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as usize] =
    //             Default::default();
    //         context.RSGetScissorRects(&mut num, std::mem::transmute(&mut rects));
    //
    //         for x in std::mem::transmute::<_, &[RECT]>(&rects[..]) {
    //             eprintln!("{:?}", x);
    //         }
    //
    //         let fp = next.fp_next();
    //         fp(this, sync, flags, next)
    //     }),
    //     |this, bufc, w, h, format, flags, mut next| {
    //         let fp = next.fp_next();
    //         fp(this, bufc, w, h, format, flags, next)
    //     },
    // )?
    // .persist();

    // let mut ipc = IpcConnectionBuilder::new(Uuid::nil());
    // let mut ipc = ipc.connect()?;
    //
    // let handle = ipc.handle();
    //
    // let ctx = OpenGLHookContext::init()?;
    //
    // let handshake = GameWindowCommand::handshake(&Uuid::nil());
    //
    // ctx.new(Box::new(move |hdc, mut next| {
    //     handle.send(handshake).unwrap_or_else(|_| println!("failed to send"));
    //     let fnext = next.fp_next();
    //     fnext(hdc, next)
    // }))?.persist();
    //
    //
    // ipc.listen()?;
    // eprintln!("ipc stop");
    Ok(())
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: HINSTANCE,
    call_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    // unsafe { DisableThreadLibraryCalls(module); }

    if call_reason == DLL_PROCESS_ATTACH {
        unsafe {
            AllocConsole();
        }

        std::thread::spawn(|| unsafe {
            println!(
                "{:?}",
                catch_unwind(|| {
                    match crate::main() {
                        Ok(()) => 0 as u32,
                        Err(e) => {
                            println!("Error occurred when injecting: {}", e);
                            1
                        }
                    }
                })
            );
            println!("over.");
        });
    }
    true.into()
}
