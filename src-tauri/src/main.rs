// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::exit,
    sync::Arc,
    thread::{self},
};

use tauri::{async_runtime::Mutex as AsyncMutex, State};

struct AppState {
    pty_pair: Arc<AsyncMutex<PtyPair>>,
    writer: Arc<AsyncMutex<Box<dyn Write + Send>>>,
    reader: Arc<AsyncMutex<BufReader<Box<dyn Read + Send>>>>,
    utf8_remainder: Arc<AsyncMutex<Vec<u8>>>,
}

fn decode_utf8_stream(buffer: &mut Vec<u8>) -> String {
    let mut output = String::new();

    loop {
        if buffer.is_empty() {
            break;
        }

        match std::str::from_utf8(buffer) {
            Ok(valid) => {
                output.push_str(valid);
                buffer.clear();
                break;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();

                if valid_up_to > 0 {
                    output.push_str(unsafe {
                        std::str::from_utf8_unchecked(&buffer[..valid_up_to])
                    });
                }

                match err.error_len() {
                    Some(invalid_len) => {
                        output.push('\u{FFFD}');
                        buffer.drain(..valid_up_to + invalid_len);
                    }
                    None => {
                        buffer.drain(..valid_up_to);
                        break;
                    }
                }
            }
        }
    }

    output
}

#[tauri::command]
// create a shell and add to it the $TERM env variable so we can use clear and other commands
async fn async_create_shell(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut cmd = CommandBuilder::new("powershell.exe");

    #[cfg(not(target_os = "windows"))]
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());
    let mut cmd = CommandBuilder::new(&shell);

    // add the $TERM env variable so we can use clear and other commands

    #[cfg(target_os = "windows")]
    cmd.env("TERM", "cygwin");

    #[cfg(not(target_os = "windows"))]
    cmd.env("TERM", "xterm-256color");

    let mut child = state
        .pty_pair
        .lock()
        .await
        .slave
        .spawn_command(cmd)
        .map_err(|err| err.to_string())?;

    thread::spawn(move || {
        let status = child.wait().unwrap();
        exit(status.exit_code() as i32)
    });
    Ok(())
}

#[tauri::command]
async fn async_write_to_pty(data: &str, state: State<'_, AppState>) -> Result<(), String> {
    write!(state.writer.lock().await, "{}", data).map_err(|e| e.to_string())
}

#[tauri::command]
async fn async_read_from_pty(state: State<'_, AppState>) -> Result<String, String> {
    let mut reader = state.reader.lock().await;
    let bytes = reader
        .fill_buf()
        .map_err(|e| e.to_string())?
        .to_vec();

    if bytes.is_empty() {
        return Ok(String::new());
    }

    let mut utf8_remainder = state.utf8_remainder.lock().await;
    utf8_remainder.extend_from_slice(&bytes);
    reader.consume(bytes.len());

    Ok(decode_utf8_stream(&mut utf8_remainder))
}

#[tauri::command]
async fn async_resize_pty(rows: u16, cols: u16, state: State<'_, AppState>) -> Result<(), String> {
    state
        .pty_pair
        .lock()
        .await
        .master
        .resize(PtySize {
            rows,
            cols,
            ..Default::default()
        })
        .map_err(|e| e.to_string())
}

fn main() {
    let pty_system = native_pty_system();

    let pty_pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let reader = pty_pair.master.try_clone_reader().unwrap();
    let writer = pty_pair.master.take_writer().unwrap();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            pty_pair: Arc::new(AsyncMutex::new(pty_pair)),
            writer: Arc::new(AsyncMutex::new(writer)),
            reader: Arc::new(AsyncMutex::new(BufReader::new(reader))),
            utf8_remainder: Arc::new(AsyncMutex::new(Vec::new())),
        })
        .invoke_handler(tauri::generate_handler![
            async_write_to_pty,
            async_resize_pty,
            async_create_shell,
            async_read_from_pty
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
