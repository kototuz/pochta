use std::collections::HashMap;
use std::ffi::{c_int, c_char, c_short, CStr};
use std::io::{Write, stdout};
use std::process::ExitCode;
use std::path::Path;

mod curl;
use curl::CurlEasy;
use curl::RecvResult;

use tinyjson::JsonValue;
use base64::prelude::*;

use rustyline::error::ReadlineError;

const CLIENT_ID:     &str = env!("CLIENT_ID");
const CLIENT_SECRET: &str = env!("CLIENT_SECRET");
const REFRESH_TOKEN: &str = env!("REFRESH_TOKEN");
const USER:          &str = env!("EMAIL");

struct GmailClient {
    curl:     CurlEasy,
    tag:      u32,
    sockfd:   c_int,
    resp_buf: [u8; 1024]
}

#[repr(C)]
struct Pollfd {
    fd:      c_int,
    events:  c_short,
    revents: c_short,
}

#[link(name = "c")]
unsafe extern "C" {
    #[link_name = "__errno_location"]
    fn errno() -> *const c_int;

    fn strerror(err: c_int) -> *const c_char;
    fn poll(pollfds: *mut Pollfd, count: u64, timeout: c_int) -> c_int;
}

impl GmailClient {
    fn connect() -> Option<Self> {
        let curl = CurlEasy::init()?;
        curl.set_url(c"imaps://imap.gmail.com:993")?;
        curl.set_connect_only()?;
        curl.perform()?;
        Some(Self {
            sockfd:   curl.get_sockfd()?,
            curl:     curl,
            tag:      0,
            resp_buf: [0u8; 1024],
        })
    }

    fn wait_for_input(&self) -> Option<()> {
        const POLLIN: c_short = 1;
        let mut p = Pollfd { fd: self.sockfd, events: POLLIN, revents: 0 };
        unsafe {
            if poll(&mut p, 1, -1) == -1 {
                let err_str = CStr::from_ptr(strerror(*errno())).to_str().unwrap();
                eprintln!("error: poll: {}", err_str);
                None
            } else {
                Some(())
            }
        }
    }

    fn recv_all(&mut self, buf: &mut Vec<u8>, tag: &[u8]) -> Option<()> {
        self.wait_for_input()?;
        loop {
            match self.curl.recv(&mut self.resp_buf)? {
                RecvResult::Ok(recv) => buf.extend_from_slice(&self.resp_buf[..recv]),
                RecvResult::Again => {
                    if buf.ends_with(b"\r\n") {
                        let last_str_begin = buf.iter()
                            .take(buf.len() - 2)
                            .rposition(|s| *s == b'\n')
                            .map(|pos| pos+1)
                            .unwrap_or(0);

                        let last_str = &buf[last_str_begin..];
                        if last_str.starts_with(tag) || last_str.starts_with(b"+") {
                            break;
                        }
                    }

                    self.wait_for_input()?;
                }
            }
        }

        Some(())
    }

    fn send_cmd(&mut self, cmd: &str, resp: &mut Vec<u8>) -> Option<()> {
        resp.clear();
        let tag = format!("K{:04}", self.tag);
        self.curl.send(&format!("{tag} {cmd}\r\n").as_bytes())?;
        self.recv_all(resp, &tag.as_bytes())?;
        self.tag += 1;
        Some(())
    }

    fn send_raw_lit(&mut self, lit: &[u8], resp: &mut Vec<u8>) -> Option<()> {
        resp.clear();
        self.curl.send(lit)?;
        self.curl.send(b"\r\n")?;
        let tag = format!("K{:04}", self.tag-1);
        self.recv_all(resp, &tag.as_bytes())?;
        Some(())
    }
}

fn get_new_auth_string() -> Option<String> {
    let curl = CurlEasy::init()?;
    curl.set_url(c"https://accounts.google.com/o/oauth2/token");

    let mut params = String::new();
    curl.query_string(&mut params, &[
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
        ("refresh_token", REFRESH_TOKEN),
        ("grant_type", "refresh_token"),
    ]);

    curl.set_post_fields(&mut params)?;

    let mut resp = Vec::<u8>::new();
    curl.set_write_data(&mut resp)?;

    curl.perform()?;

    // Request new access token
    // TODO: Maybe error reporting, but it will crash only if google change
    //       response
    let text = std::str::from_utf8(&resp).unwrap();
    let json: JsonValue = text.parse().unwrap();
    let json: &HashMap<String, JsonValue> = json.get().unwrap();
    let access_token: &String = json["access_token"].get().unwrap();

    // Construct and encode new authorization string
    let auth_string = format!("user={}\x01auth=Bearer {}\x01\x01", USER, access_token);
    let auth_string = BASE64_STANDARD.encode(auth_string.as_bytes());

    Some(auth_string)
}

fn main2() -> Option<()> {
    let auth_string = get_new_auth_string()?;
    let mut gmail = GmailClient::connect()?;
    let mut resp = Vec::<u8>::new();
    let mut stdout = stdout();

    // Authenticate using google xoauth2
    gmail.send_cmd("AUTHENTICATE XOAUTH2", &mut resp)?;
    assert!(resp.starts_with(b"+"));
    gmail.send_raw_lit(&auth_string.as_bytes(), &mut resp)?;
    stdout.write_all(&resp).unwrap();

    // Init rustyline
    let mut rl = match rustyline::DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("error: could not init 'rustyline': {e}");
            return None;
        }
    };

    // Load history file. Create if it does not exist
    let history_path = Path::new(concat!(env!("HOME"), "/.pochta/history.txt"));
    match rl.load_history(history_path) {
        Err(ReadlineError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("info: history file does not exist - creating history file '~/.pochta/history.txt'");
            if let Err(e) = std::fs::create_dir_all(history_path.parent().unwrap()) {
                eprintln!("error: could not create history file: {e}");
            }
            if let Err(e) = std::fs::File::create(history_path) {
                eprintln!("error: could not create history file: {e}");
            }
        },
        Err(e) => {
            eprintln!("error: could not load history: {e}");
        },
        _ => {}
    }

    // Main loop
    loop {
        match rl.readline(">> ") {
            Ok(line) => {
                if let Err(e) = rl.add_history_entry(&line) {
                    eprintln!("error: could not add entry to history: {e}");
                }

                gmail.send_cmd(&line, &mut resp)?;
                stdout.write_all(&resp).unwrap();
            },
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                gmail.send_cmd("LOGOUT", &mut resp)?;
                stdout.write_all(&resp).unwrap();
                break;
            },
            Err(err) => {
                eprintln!("error: rustyline: {err}");
                return None;
            },
        }
    }

    if let Err(e) = rl.save_history(history_path) {
        eprintln!("error: could not append to history: {e}");
    }

    Some(())
}

fn main() -> ExitCode {
    match main2() {
        Some(_) => ExitCode::SUCCESS,
        None    => ExitCode::FAILURE
    }
}

// TODO: Shortcut system
// TODO: Integration with browsers (to open html) and editors (convenience)
// TODO: Ability to store multiple clients?
