fn main() -> Result<(), Box<dyn Error>> {
    use dll_syringe::{Process, Syringe};
    use std::env::args;

    let _args: Vec<String> = args().collect();
    let child = Command::new("E:\\Emulators\\RetroArch\\retroarch.exe").spawn()?;
    let target_process = Process::from_child(child);
    let syringe = Syringe::new();

    syringe.inject(
        &target_process,
        "D:\\coding\\snowflake-ingame\\target\\debug\\snowflake_ingame.dll",
    )?;
    Ok(())
}

use std::error::Error;
#[cfg(not(target_os = "windows"))]
use std::io::Read;
use std::process::Command;

#[cfg(not(target_os = "windows"))]
fn main() {
    use std::env::vars;
    println!("Hello World");
    for (k, v) in vars() {
        println!("{} {}", k, v);
    }
    let _input = std::io::stdin()
        .bytes()
        .next()
        .and_then(|result| result.ok());
}
