use std::ffi::{c_void, c_int, c_char, c_long, CStr};

pub struct CurlEasy(*const c_void);

pub enum RecvResult {
    Ok(usize), // usize: received byte count
    Again      // CURLE_AGAIN
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

type CurlCode = c_int;
const CURLE_OK: c_int = 0;

#[link(name = "curl")]
unsafe extern "C" {
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
                Some(Self(res))
            }
        }
    }

    pub fn set_url(&self, url: &CStr) -> Option<()> {
        unsafe {
            let res = curl_easy_setopt(self.0, CurlOption::Url, url.as_ptr());
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
                let encoded = curl_easy_escape(self.0, param.1.as_ptr() as *const c_char, param.1.len() as c_int);
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
            let mut res = curl_easy_setopt(self.0, CurlOption::PostFieldSize, fields.len() as c_long);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                return None;
            }

            res = curl_easy_setopt(self.0, CurlOption::PostFields, fields.as_ptr() as *mut c_char);
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
            let mut res = curl_easy_setopt(self.0, CurlOption::WriteFunction, Self::write_callback as *const c_void);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                return None;
            }

            res = curl_easy_setopt(self.0, CurlOption::WriteData, buf);
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
            let res = curl_easy_setopt(self.0, CurlOption::ConnectOnly, 1);
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
            let res = curl_easy_getinfo(self.0, CURLINFO_ACTIVESOCKET, &mut sockfd);
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
            let res = curl_easy_send(self.0, msg.as_ptr(), msg.len(), &mut sent);
            if res != CURLE_OK {
                eprintln!("error: curl: {}", err_str!(res));
                return None;
            } else {
                assert_eq!(sent, msg.len());
                Some(())
            }
        }
    }


    pub fn recv(&mut self, buf: &mut [u8]) -> Option<RecvResult> {
        const CURLE_AGAIN: c_int   = 81;
        unsafe {
            let mut recv: usize = 0;
            let res = curl_easy_recv(self.0, buf.as_mut_ptr(), buf.len(), &mut recv);
            match res {
                CURLE_OK    => Some(RecvResult::Ok(recv)),
                CURLE_AGAIN => Some(RecvResult::Again),
                _ => {
                    eprintln!("error: curl: {}", err_str!(res));
                    None
                }
            }
        }
    }

    pub fn perform(&self) -> Option<()> {
        unsafe {
            let res = curl_easy_perform(self.0);
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
            curl_easy_cleanup(self.0);
        }
    }
}
