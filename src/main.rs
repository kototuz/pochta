use std::collections::HashMap;
use std::process::ExitCode;

use tinyjson::JsonValue;
use base64::prelude::*;

#[allow(unused)]
mod curl;
use curl::CurlEasy;

mod flag;
use flag::Flags;

mod client;
use client::Client;
use client::ImapClient;
use client::SmtpClient;

mod console;

const CLIENT_ID:     &str = env!("CLIENT_ID");
const CLIENT_SECRET: &str = env!("CLIENT_SECRET");
const REFRESH_TOKEN: &str = env!("REFRESH_TOKEN");
const USER:          &str = env!("EMAIL");

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
        imap> select inbox
        <response>

    Send several commands at once (it's useful for writing mail):
        smtp> data
        smtp> \"
        > Subject: Test
        >
        > Hello, world
        > .
        > \"
        <response>

    Decode command response using base64 decoder tool:
        imap> !b64 fetch 1 body[1]
        <decoded response>

    Decode command response using quoted-printable decoder tool:
        imap> !qp fetch 1 body[1]
        <decoded response>

    Open the command response in browser (ensure that env variable 'BROWSER' is set):
        imap> !b fetch 1 body[text]
        <response>

    Chain tools (!b <- !qp <-):
        imap> !b!b64 fetch 1 body[header]
        <decoded response>
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

    // Init rustyline
    let rl = match rustyline::DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("error: could not init 'rustyline': {e}");
            return None;
        }
    };

    let client: &mut dyn Client = if smtp_flag {
        &mut SmtpClient::connect()?
    } else {
        &mut ImapClient::connect()?
    };

    console::run(
        rl,
        client,
        get_new_auth_string()?,
        if history_file_flag {
            Some(concat!(env!("HOME"), "/.pochta/history.txt"))
        } else {
            None
        },
        prompt_color
    )
}

fn main() -> ExitCode {
    match main2() {
        Some(_) => ExitCode::SUCCESS,
        None    => ExitCode::FAILURE
    }
}

// TODO: Shortcut system
// TODO: Ability to store multiple clients?
