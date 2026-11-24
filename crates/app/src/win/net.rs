//! HTTPSでファイルを1つ取ってくる(WinHTTP). 今は辞書を取ってくるのに使う(K5, K31).
//! プロキシはWindowsの設定に従う. 会社のプロキシが認証を求めたら, ログインしている人の資格情報で一度だけ答える.

use super::util::wide;
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

/// 取ってくる大きさの上限(辞書は約5MB)
const MAX_BYTES: usize = 64 << 20;

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn opened(h: *mut c_void, what: &str) -> Result<Handle, String> {
    if h.is_null() {
        Err(format!("{what}: {}", windows::core::Error::from_thread()))
    } else {
        Ok(Handle(h))
    }
}

/// `https://{host}{path}`を取ってくる. 200以外は失敗とする
pub fn get(host: &str, path: &str) -> Result<Vec<u8>, String> {
    unsafe {
        let session = opened(WinHttpOpen(w!("Hikidashi"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0), "open")?;
        let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000);
        let host_w = wide(host);
        let conn = opened(WinHttpConnect(session.0, PCWSTR(host_w.as_ptr()), 443, 0), "connect")?;
        let path_w = wide(path);
        let req = opened(
            WinHttpOpenRequest(conn.0, w!("GET"), PCWSTR(path_w.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE),
            "request",
        )?;
        // 社内のプロキシ(NTLMなど)には, ログインしている人の資格情報で答えてよい
        let policy = WINHTTP_AUTOLOGON_SECURITY_LEVEL_LOW.to_ne_bytes();
        let _ = WinHttpSetOption(Some(req.0), WINHTTP_OPTION_AUTOLOGON_POLICY, Some(&policy));

        let mut code = 0;
        for attempt in 0..2 {
            WinHttpSendRequest(req.0, None, None, 0, 0, 0).map_err(|e| format!("send: {e}"))?;
            WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|e| format!("receive: {e}"))?;
            code = status_code(req.0);
            if code != 407 || attempt == 1 || !answer_proxy(req.0) {
                break;
            }
        }
        if code != 200 {
            return Err(format!("HTTP {code}"));
        }

        let mut out: Vec<u8> = Vec::new();
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(req.0, &mut avail).map_err(|e| format!("read: {e}"))?;
            if avail == 0 {
                break;
            }
            let start = out.len();
            out.resize(start + avail as usize, 0);
            let mut read = 0u32;
            WinHttpReadData(req.0, out[start..].as_mut_ptr() as *mut c_void, avail, &mut read).map_err(|e| format!("read: {e}"))?;
            out.truncate(start + read as usize);
            if read == 0 {
                break;
            }
            if out.len() > MAX_BYTES {
                return Err("too large".into());
            }
        }
        Ok(out)
    }
}

fn status_code(req: *mut c_void) -> u32 {
    let mut code = 0u32;
    let mut len = 4u32;
    unsafe {
        let _ = WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut code as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        );
    }
    code
}

/// プロキシの認証(407)に, ログインしている人の資格情報で答える. 答えられる方式がなければfalse
fn answer_proxy(req: *mut c_void) -> bool {
    let (mut supported, mut first, mut target) = (0u32, 0u32, 0u32);
    unsafe {
        if WinHttpQueryAuthSchemes(req, &mut supported, &mut first, &mut target).is_err() {
            return false;
        }
        let scheme = [WINHTTP_AUTH_SCHEME_NEGOTIATE, WINHTTP_AUTH_SCHEME_NTLM].into_iter().find(|s| supported & s.0 != 0);
        let Some(scheme) = scheme else { return false };
        WinHttpSetCredentials(req, WINHTTP_AUTH_TARGET_PROXY, scheme.0, PCWSTR::null(), PCWSTR::null(), std::ptr::null_mut()).is_ok()
    }
}
