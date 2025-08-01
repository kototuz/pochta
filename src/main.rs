use std::collections::HashMap;
use std::ffi::{c_int, c_char, c_short, CStr};
use std::io::{Write, stdout};
use std::process::ExitCode;
use std::path::Path;

#[allow(unused)]
mod curl;
use curl::CurlEasy;
use curl::RecvResult;

mod flag;
use flag::Flags;

use tinyjson::JsonValue;
use base64::prelude::*;

use rustyline::error::ReadlineError;

const CLIENT_ID:     &str = env!("CLIENT_ID");
const CLIENT_SECRET: &str = env!("CLIENT_SECRET");
const REFRESH_TOKEN: &str = env!("REFRESH_TOKEN");
const USER:          &str = env!("EMAIL");

trait Client {
    fn send_cmd(&mut self, cmd: &str, resp: &mut Vec<u8>) -> Option<()>;
    fn send_quit_cmd(&mut self, resp: &mut Vec<u8>) -> Option<()>;
    fn send_cmd_not_wait_resp(&mut self, cmd: &str) -> Option<()>;
    fn recv_all(&mut self, buf: &mut Vec<u8>) -> Option<()>;
    fn sockfd(&self) -> c_int;
    fn auth(&mut self, auth_string: &str, resp: &mut Vec<u8>) -> Option<()>;
    fn prompt_str(&self) -> &'static str;

    fn wait_for_input(&self) -> Option<()> {
        const POLLIN: c_short = 1;
        let mut p = Pollfd { fd: self.sockfd(), events: POLLIN, revents: 0 };
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
}

struct ImapClient {
    curl:     CurlEasy,
    sockfd:   c_int,
    resp_buf: [u8; 1024]
}

struct SmtpClient {
    curl:     CurlEasy,
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

impl ImapClient {
    const TAG: &[u8] = b"POCHTA ";

    fn connect() -> Option<Self> {
        let curl = CurlEasy::init()?;
        curl.set_url(c"imaps://imap.gmail.com:993")?;
        curl.set_connect_only()?;
        curl.perform()?;
        Some(Self {
            sockfd:   curl.get_sockfd()?,
            curl:     curl,
            resp_buf: [0u8; 1024],
        })
    }
}

impl Client for ImapClient {
    fn prompt_str(&self) -> &'static str {
        "imap> "
    }

    fn auth(&mut self, auth_string: &str, resp: &mut Vec<u8>) -> Option<()> {
        self.send_cmd("AUTHENTICATE XOAUTH2", resp)?;
        assert!(resp.starts_with(b"+"));
        self.curl.send(&auth_string.as_bytes())?;
        self.curl.send(b"\r\n")?;
        self.recv_all(resp)?;
        Some(())
    }

    fn sockfd(&self) -> c_int {
        self.sockfd
    }

    fn recv_all(&mut self, buf: &mut Vec<u8>) -> Option<()> {
        buf.clear();
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
                        if last_str.starts_with(Self::TAG) || last_str.starts_with(b"+") {
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
        self.curl.send(Self::TAG)?;
        self.curl.send(&cmd.as_bytes())?;
        self.curl.send(b"\r\n")?;
        self.recv_all(resp)?;
        Some(())
    }

    fn send_cmd_not_wait_resp(&mut self, cmd: &str) -> Option<()> {
        self.curl.send(Self::TAG)?;
        self.curl.send(&cmd.as_bytes())?;
        self.curl.send(b"\r\n")?;
        Some(())
    }

    fn send_quit_cmd(&mut self, resp: &mut Vec<u8>) -> Option<()> {
        self.send_cmd("LOGOUT", resp)?;
        Some(())
    }
}

impl SmtpClient {
    fn connect() -> Option<Self> {
        let curl = CurlEasy::init()?;
        curl.set_url(c"smtps://smtp.gmail.com:465")?;
        curl.set_connect_only()?;
        curl.perform()?;
        Some(Self {
            sockfd:   curl.get_sockfd()?,
            curl:     curl,
            resp_buf: [0u8; 1024],
        })
    }
}

impl Client for SmtpClient {
    fn prompt_str(&self) -> &'static str {
        "smtp> "
    }

    fn auth(&mut self, auth_string: &str, resp: &mut Vec<u8>) -> Option<()> {
        self.send_cmd(&format!("AUTH XOAUTH2 {auth_string}"), resp)?;
        Some(())
    }

    fn sockfd(&self) -> c_int {
        self.sockfd
    }

    fn recv_all(&mut self, buf: &mut Vec<u8>) -> Option<()> {
        buf.clear();
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
                        assert!(last_str[0].is_ascii_digit());
                        assert!(last_str[1].is_ascii_digit());
                        assert!(last_str[2].is_ascii_digit());

                        if last_str[3] == b' ' {
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
        self.curl.send(&cmd.as_bytes())?;
        self.curl.send(b"\r\n")?;
        self.recv_all(resp)?;
        Some(())
    }

    fn send_cmd_not_wait_resp(&mut self, cmd: &str) -> Option<()> {
        self.curl.send(&cmd.as_bytes())?;
        self.curl.send(b"\r\n")?;
        Some(())
    }

    fn send_quit_cmd(&mut self, resp: &mut Vec<u8>) -> Option<()> {
        self.send_cmd("QUIT", resp)?;
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
    // Parse command line flags
    let mut flags = Flags::parse()?;
    let history_file_flag = flags.flag_bool("history-file", "Save commands to history file '~/.pochta/history.txt'", false)?;
    let help_flag = flags.flag_bool("help", "Print this help", false)?;
    let smtp_flag = flags.flag_bool("smtp", "Connect to SMTP server (send emails) instead of IMAP (retrieve emails)", false)?;
    let mut prompt_color = flags.flag_str("prompt-color", "Set color of prompt: red|green|blue", "green")?;
    flags.check()?;

    if help_flag {
        flags.print_flags();
        print!("
usage:
    Send single command:
        imap>> select inbox
        <response>

    Send several commands at once (it's useful for writing mail):
        imap>> \"
        > noop
        > noop
        > \"
        <response>
");
        return Some(());
    }
    
    // Convert color name to escape sequence
    match prompt_color.as_str() {
        "red" => {
            prompt_color.clear();
            prompt_color.push_str("\x1b[31m")
        },
        "green" => {
            prompt_color.clear();
            prompt_color.push_str("\x1b[32m")
        },
        "blue" => {
            prompt_color.clear();
            prompt_color.push_str("\x1b[34m")
        },
        _ => {
            eprintln!("error: invalid prompt color: {prompt_color}");
            return None;
        }
    }

    let mut load_history: fn(rl: &mut rustyline::DefaultEditor, path: &Path) = |_,_| {};
    let mut save_history: fn(rl: &mut rustyline::DefaultEditor, path: &Path) = |_,_| {};
    if history_file_flag {
        load_history = |rl, path| {
            match rl.load_history(path) {
                Err(ReadlineError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    println!("info: history file does not exist - creating history file '~/.pochta/history.txt'");
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
        };

        save_history = |rl, path| {
            if let Err(e) = rl.save_history(path) {
                eprintln!("error: could not append to history: {e}");
            }
        };
    }

    let client: &mut dyn Client = if smtp_flag {
        &mut SmtpClient::connect()?
    } else {
        &mut ImapClient::connect()?
    };

    // Authenticate
    let mut server_resp = Vec::<u8>::new();
    let mut stdout = stdout();
    let auth_string = get_new_auth_string()?;
    client.auth(&auth_string, &mut server_resp)?;
    stdout.write_all(&server_resp).unwrap();

    // Init rustyline
    let mut rl = match rustyline::DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("error: could not init 'rustyline': {e}");
            return None;
        }
    };

    let history_path = Path::new(concat!(env!("HOME"), "/.pochta/history.txt"));
    load_history(&mut rl, &history_path);

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
                if multiline_mode {
                    if bytes.len() == 1 && bytes[0] == b'"' {
                        multiline_mode = false;
                        curr_prompt = &default_prompt;
                        client.recv_all(&mut server_resp)?;
                        stdout.write_all(&server_resp).unwrap();
                    } else {
                        client.send_cmd_not_wait_resp(&line)?;
                    }
                } else {
                    if bytes.len() == 1 && bytes[0] == b'"' {
                        multiline_mode = true;
                        curr_prompt = &multiline_prompt;
                    } else {
                        client.send_cmd(&line, &mut server_resp)?;
                        stdout.write_all(&server_resp).unwrap();
                    }
                }
            },
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                client.send_quit_cmd(&mut server_resp)?;
                stdout.write_all(&server_resp).unwrap();
                break;
            },
            Err(err) => {
                eprintln!("error: rustyline: {err}");
                return None;
            },
        }
    }

    save_history(&mut rl, &history_path);

    Some(())
}

fn main() -> ExitCode {
    match main2() {
        Some(_) => ExitCode::SUCCESS,
        None    => ExitCode::FAILURE
    }
}

// TODO: Embed decoders to the repl (base64, quoted-printable)
// TODO: Shortcut system
// TODO: Integration with browsers (to open html) and editors (convenience)
// TODO: Ability to store multiple clients?
