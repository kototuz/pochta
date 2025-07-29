use std::collections::HashMap;
use std::ffi::c_int;
use std::io::{Write, stdout, stdin};
use std::process::ExitCode;

mod curl;
use curl::CurlEasy;

use tinyjson::JsonValue;
use base64::prelude::*;

const CLIENT_ID:     &str = env!("CLIENT_ID");
const CLIENT_SECRET: &str = env!("CLIENT_SECRET");
const REFRESH_TOKEN: &str = env!("REFRESH_TOKEN");
const USER:          &str = env!("EMAIL");

struct GmailClient {
    curl:   CurlEasy,
    tag:    u32,
    sockfd: c_int,
}

impl GmailClient {
    fn connect() -> Option<Self> {
        let curl = CurlEasy::init()?;
        curl.set_url(c"imaps://imap.gmail.com:993")?;
        curl.set_connect_only()?;
        curl.perform()?;
        Some(Self {
            sockfd: curl.get_sockfd()?,
            curl:   curl,
            tag:    0,
        })
    }

    fn send_cmd(&mut self, cmd: &str, resp: &mut Vec<u8>) -> Option<()> {
        resp.clear();
        self.curl.send(&format!("K{:04x} {cmd}\r\n", self.tag).as_bytes())?;
        self.curl.recv(resp, self.sockfd)?;
        self.tag += 1;
        Some(())
    }

    fn send_raw_lit(&mut self, lit: &[u8], resp: &mut Vec<u8>) -> Option<()> {
        resp.clear();
        self.curl.send(lit)?;
        self.curl.send(b"\r\n")?;
        self.curl.recv(resp, self.sockfd)?;
        Some(())
    }
}

fn get_new_auth_string() -> Option<String> {
    let curl = CurlEasy::init()?;
    curl.set_url(c"https://accounts.google.com/o/oauth2/token");

    let mut params = curl.query_string(&[
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

    // Authenticate using google xoauth2
    gmail.send_cmd("AUTHENTICATE XOAUTH2", &mut resp)?;
    assert!(resp.starts_with(b"+"));
    gmail.send_raw_lit(&auth_string.as_bytes(), &mut resp)?;
    stdout().write_all(&resp).unwrap();

    // TODO: Sometimes only the first part of response is printed.
    //       The the second part will be printed after the next input.
    //       Idk why this happens. Maybe it's fucking rust with his locks.
    //       Can test with c print functions. For now it's not critical
    let mut input = String::new();
    loop {
        print!(">> ");
        stdout().flush().unwrap();
        if let Err(e) = stdin().read_line(&mut input) {
            eprintln!("error: could not read line: {e}");
            return None;
        }

        // TODO: Add exit on <C-d>

        input.pop();
        gmail.send_cmd(&input, &mut resp)?;
        print!("{}", std::str::from_utf8(&resp).unwrap());

        input.clear();
    }
}

fn main() -> ExitCode {
    match main2() {
        Some(_) => ExitCode::SUCCESS,
        None    => ExitCode::FAILURE
    }
}
