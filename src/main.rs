use std::collections::HashMap;
use std::process::ExitCode;
use std::path::PathBuf;
use std::io::{Read, Write};

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


const POCHTA_DIR: &'static str = concat!(env!("HOME"), "/.config/pochta");
const REDIRECT_URI: &str =  "https://google.github.io/gmail-oauth2-tools/html/oauth2.dance.html";

type Profile = HashMap<String, JsonValue>;

fn get_new_auth_string(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
    email: &str,
) -> Option<String> {
    let curl = CurlEasy::init()?;
    curl.set_url(c"https://accounts.google.com/o/oauth2/token");

    let mut params = String::new();
    curl.query_string(&mut params, &[
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ]);

    curl.set_post_fields(&mut params)?;

    let mut resp = Vec::<u8>::new();
    curl.set_write_data(&mut resp)?;

    // Request new access token
    curl.perform()?;

    // This will fail only if google change the response
    let text = std::str::from_utf8(&resp).unwrap();
    let json: JsonValue = text.parse().unwrap();
    let json: &HashMap<String, JsonValue> = json.get().unwrap();

    let access_token: &String = match json.get("access_token") {
        Some(v) => v.get().unwrap(),
        None => {
            eprintln!("error: could not fetch access token\n");
            eprintln!("Check your profile. Maybe refresh token is outdated");
            return None;
        }
    };

    // Construct and encode new authorization string
    let auth_string = format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token);
    let auth_string = BASE64_STANDARD.encode(auth_string.as_bytes());

    Some(auth_string)
}

fn profile_expect_str(p: &Profile, name: &'static str) -> Option<String> {
    match p.get(name) {
        Some(v) => {
            match v.get::<String>() {
                Some(v) => {
                    if v.is_empty() {
                        eprintln!("error: entry '{name}' must not be empty");
                        None
                    } else {
                        Some(v.clone())
                    }
                }
                None => {
                    eprintln!("error: profile: entry '{name}' must have string type");
                    None
                }
            }
        },
        None => {
            eprintln!("error: profile: provide '{name}' entry");
            None
        }
    }
}

fn profile_expect_bool(p: &Profile, name: &'static str) -> Option<bool> {
    match p.get(name) {
        Some(v) => {
            match v.get::<bool>() {
                Some(v) => Some(*v),
                None => {
                    eprintln!("error: profile: entry '{name}' must have bool type");
                    None
                }
            }
        },
        None => {
            eprintln!("error: profile: provide '{name}' entry");
            None
        }
    }
}

fn update_refresh_token(
    profile: &mut Profile,
    rl: &mut rustyline::DefaultEditor,
    client_id: &str,
    client_secret: &str,
) -> Option<()> {
    let curl = CurlEasy::init()?;

    // Construct the URL for authorizing access
    let mut url = String::new();
    url.push_str("https://accounts.google.com/o/oauth2/auth?");
    curl.query_string(&mut url, &[
        ("client_id", client_id),
        ("redirect_uri", REDIRECT_URI),
        ("scope", "https://mail.google.com/"),
        ("response_type", "code"),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ]);

    // Get verification code
    println!("[Authorize token]: {url}");
    let verif_code = match rl.readline("[Enter verification code]: ") {
        Ok(code) => code,
        _ => {
            println!("aborting...");
            return None;
        }
    };

    // Construct url for obtaining oauth refresh token
    curl.set_url(c"https://accounts.google.com/o/oauth2/token")?;
    url.clear();
    curl.query_string(&mut url, &[
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", &verif_code),
        ("redirect_uri", REDIRECT_URI),
        ("grant_type", "authorization_code"),
    ]);
    curl.set_post_fields(&mut url)?;

    // Set buffer for response
    let mut resp = Vec::<u8>::new();
    curl.set_write_data(&mut resp)?;

    curl.perform()?;

    // Extract refresh token from response
    let text = std::str::from_utf8(&resp).unwrap();
    let json: JsonValue = text.parse().unwrap();
    let json: &HashMap<String, JsonValue> = json.get().unwrap();
    let new_refresh_token: &String = match json.get("refresh_token") {
        Some(token) => token.get().unwrap(),
        None => {
            eprintln!("error: refresh token is not found in the response; ensure that you did everything right");
            return None;
        }
    };

    let refresh_token = profile.get_mut("refresh_token").unwrap();
    *refresh_token = JsonValue::from(new_refresh_token.clone());

    Some(())
}

fn main2() -> Option<()> {
    let mut flags = Flags::parse()?;
    let help_flag = flags.flag_bool("help", "Print this help", false)?;
    let help_usage_flag = flags.flag_bool("help-usage", "Print help about how to use this program", false)?;
    let help_profile_flag = flags.flag_bool("help-profile", "Print help about how to setup profile", false)?;
    let smtp_flag = flags.flag_bool("smtp", "Connect to SMTP server (send emails) instead of IMAP (retrieve emails)", false)?;
    let new_profile = flags.flag_str("new-profile", "Create new profile", "")?;
    let profile = flags.flag_str("p", "Use this profile", "")?;
    let update_refresh_token_flag = flags.flag_bool("update-refresh-token", "Update refresh token for this profile", false)?;
    flags.check()?;

    if help_flag {
        flags.print_flags();
        return Some(());
    }

    if help_usage_flag {
        print!(
"usage:
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
"
        );

        return Some(());
    }

    if help_profile_flag {
        println!("1. Create new profile: 'pochta -new-profile <name>'");
        println!("2. Enter your email into profile");
        println!("3. Create new project using google developer console: https://console.developers.google.com.");
        println!("   Find client id and client secret there and enter these values into your profile.");
        println!("4. Add yourself as a tester of the project.");
        println!("5. Find OAuth 'Web client' or create a new one and add this redirect uri: {REDIRECT_URI}");
        println!("6. Run: 'pochta -p <create_profile> -update-refresh-token'");
        println!("7. Here you go");
        return Some(());
    }

    let mut path_buf = PathBuf::from(POCHTA_DIR);

    // Create new profile
    if !new_profile.is_empty() {
        if !std::fs::exists(POCHTA_DIR).unwrap() {
            if let Err(e) = std::fs::create_dir_all(POCHTA_DIR) {
                eprintln!("error: could not create '{POCHTA_DIR}': {e}");
                return None;
            }
        }

        path_buf.push(new_profile);
        path_buf.set_extension("json");

        let mut profile_file = match std::fs::File::create_new(&path_buf) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error: could not create new profile: {e}");
                return None;
            }
        };

        profile_file.write_all(
b"{
  \"client_id\": \"REPLACE_ME\",
  \"client_secret\": \"REPLACE_ME\",
  \"email\": \"REPLACE_ME\",
  \"refresh_token\": \"AUTO-GENERATED\",
  \"use_history_file\": false,
  \"prompt_color\": \"green\"
}"
        ).unwrap();

        println!("new profile is generated at '{}'", path_buf.display());

        return Some(());
    }

    if profile.is_empty() {
        eprintln!("error: profile must be provided");
        return None;
    }

    path_buf.push(profile);
    path_buf.set_extension("json");

    // Parse profile
    let mut profile: Profile;
    let client_id: String;
    let client_secret: String;
    let refresh_token: String;
    let email: String;
    let use_history_file: bool;
    let mut prompt_color: String;
    {
        let mut profile_file = match std::fs::File::open(&path_buf) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("error: could not open profile: {e}");
                return None;
            }
        };

        let mut profile_str = String::new();
        match profile_file.read_to_string(&mut profile_str) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("error: could not read profile: {e}");
                return None;
            }
        }

        let json: JsonValue = match profile_str.parse() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("error: could not parse profile: {e}");
                return None;
            }
        };

        profile = match json.try_into() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("error: invalid profile structure");
                return None;
            }
        };

        client_id = profile_expect_str(&profile, "client_id")?;
        client_secret = profile_expect_str(&profile, "client_secret")?;
        refresh_token = profile_expect_str(&profile, "refresh_token")?;
        email = profile_expect_str(&profile, "email")?;
        use_history_file = profile_expect_bool(&profile, "use_history_file")?;
        prompt_color = profile_expect_str(&profile, "prompt_color")?;
    }

    // Init rustyline
    let mut rl = match rustyline::DefaultEditor::new() {
        Ok(rl) => rl,
        Err(e) => {
            eprintln!("error: could not init 'rustyline': {e}");
            return None;
        }
    };

    if update_refresh_token_flag {
        update_refresh_token(
            &mut profile,
            &mut rl,
            &client_id,
            &client_secret,
        )?;

        // Update profile file
        if let Err(e) = std::fs::write(path_buf, JsonValue::from(profile).format().unwrap().as_bytes()) {
            eprintln!("error: could not update profile: {e}");
            return None;
        }

        return Some(());
    }

    // Convert color name to escape sequence
    match prompt_color.as_str() {
        "red" => {
            prompt_color.clear();
            prompt_color.push_str("\x1b[31m");
        },
        "green" => {
            prompt_color.clear();
            prompt_color.push_str("\x1b[32m");
        },
        "blue" => {
            prompt_color.clear();
            prompt_color.push_str("\x1b[34m");
        },
        _ => {
            eprintln!("error: invalid prompt color: {prompt_color}");
            return None;
        }
    }

    let client: &mut dyn Client = if smtp_flag {
        &mut SmtpClient::connect()?
    } else {
        &mut ImapClient::connect()?
    };

    console::run(
        rl,
        client,
        get_new_auth_string(&client_id, &client_secret, &refresh_token, &email)?,
        if use_history_file {
            Some(concat!(env!("HOME"), "/.config/pochta/history.txt"))
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
