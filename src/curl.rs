use std::ffi::{c_void, c_int, c_char, c_long, c_short, CStr};

pub struct CurlEasy {
    curl:     *const c_void,
    recv_buf: [u8; 1024]
}

#[repr(C)]
enum CurlOption {
    Url           = 10002,
    PostFields    = 10015,
    PostFieldSize = 60,
    WriteFunction = 20011,
    WriteData     = 10001,
    ConnectOnly   = 141,
}

#[repr(C)]
struct Pollfd {
    fd:      c_int,
    events:  c_short,
    revents: c_short,
}

type CurlCode = c_int;
const CURLE_OK: c_int = 0;

#[link(name = "curl")]
#[link(name = "c")]
unsafe extern "C" {
    #[link_name = "__errno_location"]
    fn errno() -> *const c_int;

    fn strerror(err: c_int) -> *const c_char;
    fn poll(pollfds: *mut Pollfd, count: u64, timeout: c_int) -> c_int;

    fn curl_easy_init() -> *const c_void;
    fn curl_easy_cleanup(curl: *const c_void);
    fn curl_easy_setopt(curl: *const c_void, opt: CurlOption, ...) -> CurlCode;
    fn curl_easy_strerror(err: CurlCode) -> *const c_char;
    fn curl_easy_escape(curl: *const c_void, string: *const c_char, len: c_int) -> *mut c_char;
    fn curl_free(ptr: *mut c_void);
    fn curl_easy_perform(curl: *const c_void) -> CurlCode;
    fn curl_easy_getinfo(curl: *const c_void, info: c_int, ...) -> CurlCode;
    fn curl_easy_send(curl: *const c_void, buf: *const u8, len: usize, sent: *mut usize) -> CurlCode;
    fn curl_easy_recv(curl: *const c_void, buf: *mut u8, len: usize, recv: *mut usize) -> CurlCode;
}

macro_rules! err_str {
    ($err:expr) => {
        CStr::from_ptr(curl_easy_strerror($err)).to_str().unwrap()
    }
}

impl CurlEasy {
    pub fn init() -> Option<Self> {
        unsafe {
            let res = curl_easy_init();
            if res.is_null() {
                eprintln!("error: curl: could not init");
                None
            } else {
                Some(Self{ curl: res, recv_buf: [0u8; 1024] })
            }
        }
    }

    pub fn set_url(&self, url: &CStr) -> Option<()> {
        unsafe {
            let res = curl_easy_setopt(self.curl, CurlOption::Url, url.as_ptr());
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                None
            } else {
                Some(())
            }
        }
    }

    pub fn query_string(&self, params: &[(&str, &str)]) -> String {
        let mut res = String::new();
        for param in params {
            res.push_str(param.0);
            res.push('=');

            unsafe {
                let encoded = curl_easy_escape(self.curl, param.1.as_ptr() as *const c_char, param.1.len() as c_int);
                res.push_str(CStr::from_ptr(encoded).to_str().unwrap());
                curl_free(encoded as *mut c_void);
            }

            res.push('&');
        }

        res.pop();
        res
    }


    pub fn set_post_fields(&self, fields: &mut str) -> Option<()> {
        unsafe {
            let mut res = curl_easy_setopt(self.curl, CurlOption::PostFieldSize, fields.len() as c_long);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                return None;
            }

            res = curl_easy_setopt(self.curl, CurlOption::PostFields, fields.as_ptr() as *mut c_char);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                None
            } else {
                Some(())
            }
        }
    }

    pub fn set_write_data(&self, buf: *mut Vec<u8>) -> Option<()> {
        unsafe {
            let mut res = curl_easy_setopt(self.curl, CurlOption::WriteFunction, Self::write_callback as *const c_void);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                return None;
            }

            res = curl_easy_setopt(self.curl, CurlOption::WriteData, buf);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                None
            } else {
                Some(())
            }
        }
    }

    pub fn set_connect_only(&self) -> Option<()> {
        unsafe {
            let res = curl_easy_setopt(self.curl, CurlOption::ConnectOnly, 1);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                None
            } else {
                Some(())
            }
        }
    }

    pub fn get_sockfd(&self) -> Option<c_int> {
        unsafe {
            const CURLINFO_ACTIVESOCKET: c_int = 5242924;
            let mut sockfd: c_int = 0;
            let res = curl_easy_getinfo(self.curl, CURLINFO_ACTIVESOCKET, &mut sockfd);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                None
            } else {
                Some(sockfd)
            }
        }
    }

    pub fn send(&self, msg: &[u8]) -> Option<()> {
        unsafe {
            let mut sent: usize = 0;
            let res = curl_easy_send(self.curl, msg.as_ptr(), msg.len(), &mut sent);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                return None;
            } else {
                assert_eq!(sent, msg.len());
                Some(())
            }
        }
    }

    pub fn recv(&mut self, buf: &mut Vec<u8>, sockfd: c_int) -> Option<()> {
        const POLLIN:      c_short = 1;
        const CURLE_AGAIN: c_int   = 81;
        unsafe {
            let mut p = Pollfd { fd: sockfd, events: POLLIN, revents: 0 };
            if poll(&mut p, 1, -1) == -1 {
                let err_str = CStr::from_ptr(strerror(*errno())).to_str().unwrap();
                eprintln!("error: poll: {}", err_str);
                return None;
            }

            loop {
                let mut recv: usize = 0;
                let res = curl_easy_recv(self.curl, self.recv_buf.as_mut_ptr(), self.recv_buf.len(), &mut recv);
                match res {
                    CURLE_AGAIN => { return Some(()) },
                    CURLE_OK    => { buf.extend_from_slice(&self.recv_buf[..recv]); },
                    _ => {
                        eprintln!("error: curl: {}", err_str!(res));
                        return None;
                    }
                }
            }
        }
    }

    pub fn perform(&self) -> Option<()> {
        unsafe {
            let res = curl_easy_perform(self.curl);
            if res != CURLE_OK {
                eprintln!("error: curl: could not perform: {}", err_str!(res));
                None
            } else {
                Some(())
            }
        }
    }

    extern "C" fn write_callback(contents: *mut c_void, size: usize, nmemb: usize, userp: *mut c_void) -> usize {
        unsafe {
            let realsize = size * nmemb;
            let buf = userp.cast::<Vec<u8>>();
            let data = std::slice::from_raw_parts(contents as *mut u8, size * nmemb);
            (*buf).extend_from_slice(data);
            realsize
        }
    }
}

impl Drop for CurlEasy {
    fn drop(&mut self) {
        unsafe {
            curl_easy_cleanup(self.curl);
        }
    }
}
