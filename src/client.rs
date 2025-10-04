use crate::curl::CurlEasy;
use crate::curl::RecvResult;

use std::ffi::{c_int, c_char, c_short, CStr};

pub trait Client {
    fn send_cmd(&mut self, cmd: &str) -> Option<()>;
    fn send_quit_cmd(&mut self) -> Option<()>;
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

    fn send_cmd_and_recv_resp(&mut self, cmd: &str, resp: &mut Vec<u8>) -> Option<()> {
        self.send_cmd(cmd)?;
        self.recv_all(resp)?;
        Some(())
    }
}

pub struct ImapClient {
    curl:     CurlEasy,
    sockfd:   c_int,
    resp_buf: [u8; 1024]
}

pub struct SmtpClient {
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

    pub fn connect() -> Option<Self> {
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
        self.send_cmd_and_recv_resp("AUTHENTICATE XOAUTH2", resp)?;
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

    fn send_cmd(&mut self, cmd: &str) -> Option<()> {
        self.curl.send(Self::TAG)?;
        self.curl.send(&cmd.as_bytes())?;
        self.curl.send(b"\r\n")?;
        Some(())
    }

    fn send_quit_cmd(&mut self) -> Option<()> {
        self.send_cmd("LOGOUT")?;
        Some(())
    }
}

impl SmtpClient {
    pub fn connect() -> Option<Self> {
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
        self.send_cmd_and_recv_resp(&format!("AUTH XOAUTH2 {auth_string}"), resp)?;
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

    fn send_cmd(&mut self, cmd: &str) -> Option<()> {
        self.curl.send(&cmd.as_bytes())?;
        self.curl.send(b"\r\n")?;
        Some(())
    }

    fn send_quit_cmd(&mut self) -> Option<()> {
        self.send_cmd("QUIT")?;
        Some(())
    }
}
