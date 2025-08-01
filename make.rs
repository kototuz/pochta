// 'make.rs' is a tool to setup 'pochta'.
// I wanted to call the file 'build.rs' but it is forbidden :(

use std::process::ExitCode;
use std::io::Write; 
use std::collections::HashMap;
use std::process::Command;

#[path = "src/curl.rs"]
#[allow(unused)]
mod curl;
use curl::CurlEasy;

#[path = "src/flag.rs"]
mod flag;
use flag::Flags;

use tinyjson::JsonValue;

const REDIRECT_URI: &str =  "https://google.github.io/gmail-oauth2-tools/html/oauth2.dance.html";

fn input(prompt: &str, buf: &mut String) {
    std::io::stdout().write_all(prompt.as_bytes()).unwrap();
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_line(buf).unwrap();
    if let Some(last_char) = buf.pop() {
        assert_eq!(last_char, '\n');
    }
}

fn main2() -> Option<()> {
    // Parse command line flags
    let mut flags = Flags::parse()?;
    let history_file_flag = flags.flag_bool("history-file", "Save commands to history file '~/.pochta/history.txt'", false)?;
    let help_flag = flags.flag_bool("help", "Print this help", false)?;
    let mut prompt_color = flags.flag_str("prompt-color", "Set color of prompt: red|green|blue", "green")?;
    flags.check()?;

    if help_flag {
        flags.print_flags();
        return Some(());
    }

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

    // Get the oauth client id and secret
    let mut client_id = String::new();
    let mut client_secret = String::new();
    println!("1. Create new project using google developer console: https://console.developers.google.com");
    println!("2. Find OAuth 'Web client' or create a new one");
    input("3. Copy the client id: ", &mut client_id);
    input("4. Copy the client secret: ", &mut client_secret);

    let curl = CurlEasy::init()?;

    // Construct the URL for authorizing access
    let mut url = String::new();
    url.push_str("https://accounts.google.com/o/oauth2/auth?");
    curl.query_string(&mut url, &[
        ("client_id", &client_id),
        ("redirect_uri", REDIRECT_URI),
        ("scope", "https://mail.google.com/"),
        ("response_type", "code"),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ]);

    // Get verification code
    let mut verif_code = String::new();
    println!("5. Add to the client this redirect uri: {REDIRECT_URI}");
    println!("6. Authorize token: {url}");
    input("7. Enter verification code: ", &mut verif_code);

    // Construct url for obtaining oauth refresh token
    curl.set_url(c"https://accounts.google.com/o/oauth2/token")?;
    url.clear();
    curl.query_string(&mut url, &[
        ("client_id", &client_id),
        ("client_secret", &client_secret),
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
    let refresh_token: &String = match json.get("refresh_token") {
        Some(token) => token.get().unwrap(),
        None => {
            eprintln!("error: refresh token is not found in the response; ensure that you did everything right");
            return None;
        }
    };

    let mut email = String::new();
    input("8. Enter your gmail address: ", &mut email);

    // Build the program
    let mut cmd = Command::new("cargo");
    cmd.args(&["build", "--release", "--bin", "pochta"]);
    cmd.env("CLIENT_ID", &client_id);
    cmd.env("CLIENT_SECRET", &client_secret);
    cmd.env("REFRESH_TOKEN", &refresh_token);
    cmd.env("EMAIL", &email);
    cmd.env("PROMPT_COLOR", &prompt_color);

    if history_file_flag {
        cmd.args(&["--features", "cmd_history_file"]);
    }

    cmd.status().expect("pochta build failed");

    Some(())
}

fn main() -> ExitCode {
    match main2() {
        Some(_) => ExitCode::SUCCESS,
        None    => ExitCode::FAILURE,
    }
}
