use std::io::stdout;
use std::path::Path;
use std::io::Write;

use rustyline::error::ReadlineError;
use base64::prelude::*;
use quoted_printable as qp;
use crate::client::Client;

fn apply_tools(mut tools: &str, buf: &mut Vec<u8>) {
    assert_eq!(tools.as_bytes()[0], b'!');
    while let Some(i) = tools.rfind('!') {
        let tool = &tools[i+1..];
        match tool {
            "b64" => {
                match BASE64_STANDARD.decode(&buf) {
                    Ok(decoded) => *buf = decoded,
                    Err(e) => {
                        eprintln!("error: could not decode: {e}");
                    }
                }
            },
            "qp" => {
                match qp::decode(&buf, qp::ParseMode::Robust) {
                    Ok(decoded) => *buf = decoded,
                    Err(e) => {
                        eprintln!("error: could not decode: {e}");
                    }
                }
            },
            "b" => {
                // Write buffer to file which will be provided to browser as html
                let mut file = match std::fs::File::create("/tmp/pochta-response.html") {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("error: could not create file '/tmp/pochta-response.html': {e}");
                        return;
                    }
                };
                file.write_all(&buf).unwrap();

                // Get browser executable
                let browser = match std::env::var("BROWSER") {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("error: could not get environment variable 'BROWSER': {e}");
                        return;
                    }
                };

                let res = std::process::Command::new(browser)
                    .arg("/tmp/pochta-response.html")
                    .status();

                if let Err(_) = res {
                    eprintln!("error: could not run browser");
                }
            },
            &_ => {
                eprintln!("error: tool '{tool}' not found");
            }
        }

        tools = &tools[..i];
    }
}

pub fn run(
    mut rl: rustyline::DefaultEditor,
    client: &mut dyn Client,
    auth_string: String,
    history_file: Option<&'static str>,
    prompt_color: String,
) -> Option<()> {
    if let Some(path) = history_file {
        let path = Path::new(path);
        match rl.load_history(path) {
                Err(ReadlineError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    println!("info: history file does not exist - creating history file '{}'", path.display());
                    if let Err(e) = std::fs::create_dir_all(path.parent().unwrap()) {
                        eprintln!("error: could not create history file: {e}");
                    }
                    if let Err(e) = std::fs::File::create(path) {
                        eprintln!("error: could not create history file: {e}");
                    }
                },
                Err(e) => {
                    eprintln!("error: could not load history: {e}");
                },
                _ => {}
        }
    }

    // Authenticate
    let mut server_resp = Vec::<u8>::new();
    let mut stdout = stdout();
    client.auth(&auth_string, &mut server_resp)?;
    stdout.write_all(&server_resp).unwrap();

    // Main loop
    let default_prompt = format!("{prompt_color}{}\x1b[0m", client.prompt_str());
    let multiline_prompt = format!("{prompt_color}> \x1b[0m");
    let mut curr_prompt = &default_prompt;
    let mut multiline_mode = false;
    loop {
        match rl.readline(curr_prompt) {
            Ok(line) => {
                if let Err(e) = rl.add_history_entry(&line) {
                    eprintln!("error: could not add entry to history: {e}");
                }

                let bytes = line.as_bytes();
                if bytes.is_empty() { continue; }

                // TODO: I think multiline mode is only applicable to 'smtp'.
                // In that way we can send '<CRLF>.<CRLF>' when exit from multiline mode.
                // '<CRLF>.<CRLF>' is a sequence to notify server about the email end.
                // Now user must type this sequence by himself. If user don't type the sequence
                // the program will break
                if multiline_mode {
                    if bytes.len() == 1 && bytes[0] == b'"' {
                        multiline_mode = false;
                        curr_prompt = &default_prompt;
                        client.recv_all(&mut server_resp)?;
                        stdout.write_all(&server_resp).unwrap();
                    } else {
                        client.send_cmd(&line)?;
                    }
                } else {
                    if bytes.len() == 1 && bytes[0] == b'"' {
                        multiline_mode = true;
                        curr_prompt = &multiline_prompt;
                    } else {
                        if bytes[0] == b'!' {
                            // Apply tools to the command response
                            if let Some(i) = line.find(' ') {
                                let tools = &line[..i];
                                let cmd = &line[i+1..];
                                client.send_cmd_and_recv_resp(cmd, &mut server_resp);
                                apply_tools(tools, &mut server_resp);
                            } else {
                                server_resp.clear();
                                eprintln!("error: command not specified");
                            }
                        } else {
                            client.send_cmd_and_recv_resp(&line, &mut server_resp)?;
                        }

                        stdout.write_all(&server_resp).unwrap();
                    }
                }
            },
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                client.send_quit_cmd()?;
                client.recv_all(&mut server_resp)?;
                stdout.write_all(&server_resp).unwrap();
                break;
            },
            Err(err) => {
                eprintln!("error: rustyline: {err}");
                return None;
            },
        }
    }

    if let Some(path) = history_file {
        let path = Path::new(path);
        if let Err(e) = rl.save_history(path) {
            eprintln!("error: could not append to history: {e}");
        }
    }

    Some(())
}
