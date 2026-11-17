//! Kabuk hizmeti taşıması: `audio` ve `notify` bunun üstünde çalışır.
//! Ana makine testleri `set_transport` ile sahte bir çekirdek takar; gerçek uygulamada `sys::shell_call` kullanılır.

use nc_abi::shell as wire;

pub type Transport = fn(&[u8], &mut [u8]) -> i64;

static mut TRANSPORT: Transport = crate::sys::shell_call;

#[doc(hidden)]
pub fn set_transport(transport: Transport) {
    unsafe {
        *core::ptr::addr_of_mut!(TRANSPORT) = transport;
    }
}

pub struct Reply {
    pub status: i32,
    buf: [u8; wire::RESP_HEADER + wire::MAX_RESPONSE],
    len: usize,
}

impl Reply {
    pub fn payload(&self) -> &[u8] {
        &self.buf[wire::RESP_HEADER..wire::RESP_HEADER + self.len]
    }

    pub fn u32_at(&self, index: usize) -> Option<u32> {
        let p = self.payload();
        let at = index * 4;
        if p.len() < at + 4 {
            return None;
        }
        Some(u32::from_le_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]]))
    }
}

pub fn call(op: u8, arg0: u32, arg1: u32, payload: &[u8]) -> Result<Reply, i32> {
    let mut request = [0u8; wire::REQ_HEADER + wire::MAX_PCM_WRITE / 2];
    let Some(total) = wire::encode_request(&mut request, op, arg0, arg1, 0, payload) else {
        return Err(wire::ERR_BAD_REQUEST);
    };
    let mut reply = Reply { status: 0, buf: [0; wire::RESP_HEADER + wire::MAX_RESPONSE], len: 0 };
    let transport = unsafe { *core::ptr::addr_of!(TRANSPORT) };
    let written = transport(&request[..total], &mut reply.buf);
    if written < 0 {
        return Err(wire::ERR_FAILED);
    }
    match wire::parse_response(&reply.buf[..written as usize]) {
        Ok((status, payload)) => {
            reply.status = status;
            reply.len = payload.len();
            Ok(reply)
        }
        Err(_) => Err(wire::ERR_FAILED),
    }
}
