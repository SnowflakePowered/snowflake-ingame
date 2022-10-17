use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::{env, fs, slice, str};

use windows::core::PCSTR;
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::ID3DBlob;

fn main() -> Result<(), Box<dyn Error>> {
    static VERTEX_SHADER: &str = include_str!("src/shaders/vertex_shader.vs_4_0");
    static PIXEL_SHADER: &str = include_str!("src/shaders/pixel_shader.ps_4_0");

    let mut vs_blob = None;
    let mut ps_blob = None;

    let mut err = None;

    unsafe {
        // Compile vertex shader
        D3DCompile(
            VERTEX_SHADER.as_ptr().cast(),
            VERTEX_SHADER.len(),
            None,
            None,
            None,
            PCSTR(b"main\0".as_ptr()),
            PCSTR(b"vs_4_0\0".as_ptr()),
            0,
            0,
            &mut vs_blob,
            Some(&mut err),
        )?;

        check_shader_err(&err)?;

        if let Some(vs_blob) = vs_blob {
            write_blob("vertex_shader.vs_4_0", vs_blob);
        }
    }

    unsafe {
        // Compile vertex shader
        D3DCompile(
            PIXEL_SHADER.as_ptr().cast(),
            PIXEL_SHADER.len(),
            None,
            None,
            None,
            PCSTR(b"main\0".as_ptr()),
            PCSTR(b"ps_4_0\0".as_ptr()),
            0,
            0,
            &mut ps_blob,
            Some(&mut err),
        )?;

        check_shader_err(&err)?;

        if let Some(ps_blob) = ps_blob {
            write_blob("pixel_shader.ps_4_0", ps_blob);
        }
    }
    Ok(())
}

unsafe fn write_blob(shader_name: &str, blob: ID3DBlob) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let data = slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize());
    let _ = fs::write(&format!("{}/{}", out_dir, shader_name), data)
        .map_err(|e| panic!("Unable to write {} shader to out dir: {:?}", shader_name, e));
}

enum D3DShaderCompilerError {
    CompilerError(String),
}

impl Debug for D3DShaderCompilerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            D3DShaderCompilerError::CompilerError(s) => f.write_str(s),
        }
    }
}

impl Display for D3DShaderCompilerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            D3DShaderCompilerError::CompilerError(s) => f.write_str(s),
        }
    }
}

impl Error for D3DShaderCompilerError {}

fn check_shader_err(err: &Option<ID3DBlob>) -> Result<(), D3DShaderCompilerError> {
    match err {
        None => Ok(()),
        Some(err) => {
            let err_msg = unsafe {
                str::from_utf8(slice::from_raw_parts(
                    err.GetBufferPointer().cast::<u8>(),
                    err.GetBufferSize(),
                ))
                .map(ToOwned::to_owned)
                .unwrap_or_else(|_| String::from("Unknown error"))
            };
            Err(D3DShaderCompilerError::CompilerError(err_msg))
        }
    }
}
