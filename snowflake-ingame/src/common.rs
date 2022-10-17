use crate::ipc::cmd::GameWindowCommand;
use imgui::{Condition, Image, StyleVar, TextureId, Ui, Window, WindowFlags};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Dimensions {
        Dimensions { width, height }
    }
}

impl From<[f32; 2]> for Dimensions {
    fn from(item: [f32; 2]) -> Self {
        Dimensions {
            width: item[0] as u32,
            height: item[1] as u32,
        }
    }
}

impl From<Dimensions> for [f32; 2] {
    fn from(item: Dimensions) -> Self {
        [item.width as f32, item.height as f32]
    }
}

impl From<D3D11_TEXTURE2D_DESC> for Dimensions {
    fn from(item: D3D11_TEXTURE2D_DESC) -> Self {
        Dimensions {
            width: item.Width,
            height: item.Height,
        }
    }
}

pub struct OverlayWindow;
impl OverlayWindow {
    pub fn new(ui: &Ui, tid: TextureId, dim: Dimensions) {
        let _style_pad = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
        let _style_border = ui.push_style_var(StyleVar::WindowBorderSize(0.0));
        // We don't care if the window isn't rendered.
        Window::new("BrowserWindow")
            .size(dim.into(), Condition::Always)
            .position([0.0, 0.0], Condition::Always)
            .flags(
                WindowFlags::NO_DECORATION
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_BACKGROUND,
            )
            .no_decoration()
            .build(ui, || Image::new(tid, dim.into()).build(ui))
            .unwrap_or(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    #[error("A IPC error has occured ({0:?}).")]
    IpcError(#[from] Box<tokio::sync::mpsc::error::SendError<GameWindowCommand>>),

    #[error("A internal OpenGL error occurred ({0:?}).")]
    OpenGLInternalError(#[from] imgui_renderer_ogl::RenderError),

    #[error("A internal Direct3D11 error occurred ({0:?}).")]
    Direct3D11InternalError(#[from] imgui_renderer_dx11::RenderError),

    #[error("A internal DXGI error occurred ({0:x?}).")]
    DXGIInternalError(#[from] windows::core::Error),

    #[error("The requested renderer has not been initialized.")]
    RendererNotReady,

    #[error("Error occurred when trying to open shared handle {0:x?} ({1:x?}).")]
    OverlayHandleError(HANDLE, windows::core::Error), // 128 + 64

    #[error("The overlay texture handle has not been initialized.")]
    OverlayHandleNotReady,

    #[error("The overlay mutex could not be acquired.")]
    OverlayMutexNotReady,

    #[error("The overlay could not be initialized. {0}")]
    OverlayPaintNotReady(Box<RenderError>),

    #[error("The ImGui context could not be readied for paint. {0}")]
    ImGuiNotReady(Box<RenderError>),

    #[error("The kernel was not properly initialized.")]
    KernelNotReady,
}
