// kernel/src/ring3/net.rs
//
// HTTP hizmeti (`SYS_NET_CALL`, sozlesme: crates/foundation/nc-abi/src/net.rs). TCP/DNS/TLS durum makinesi cekirdekte kalir;
// Ring 3 Tarayici yalnizca istek baslatir, ilerletir ve govdeyi parca parca okur. Yetki denetimi (NETWORK_CLIENT)
// cagiran taraftadir (ring3.rs). Her uygulama icin en cok `MAX_SLOTS_PER_APP` es zamanli istek tutulur.
#![allow(dead_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use nc_abi::net as wire;
use nc_net::{HttpState as KernelState, TcpHttpClient};

pub struct HttpSlot {
    pub id: u32,
    pub client: TcpHttpClient,
}

fn state_code(state: KernelState) -> wire::HttpState {
    match state {
        KernelState::Idle => wire::HttpState::Idle,
        KernelState::WaitingNetwork => wire::HttpState::WaitingNetwork,
        KernelState::ResolvingDns => wire::HttpState::ResolvingDns,
        KernelState::ConnectingSyn => wire::HttpState::ConnectingSyn,
        KernelState::Handshaking => wire::HttpState::Handshaking,
        KernelState::Connected => wire::HttpState::Connected,
        KernelState::RequestSent => wire::HttpState::RequestSent,
        KernelState::ReceivingData => wire::HttpState::ReceivingData,
        KernelState::Complete => wire::HttpState::Complete,
        KernelState::Failed => wire::HttpState::Failed,
    }
}

fn clone_str(text: &str) -> Option<String> {
    let mut out = String::new();
    out.try_reserve_exact(text.len()).ok()?;
    out.push_str(text);
    Some(out)
}

pub fn handle(request: &wire::Header<'_>, slots: &mut Vec<HttpSlot>, next_id: &mut u32, response_cap: usize) -> (i32, Vec<u8>) {
    match request.op {
        wire::OP_START => start(request, slots, next_id),
        wire::OP_POLL => poll(request, slots, response_cap),
        wire::OP_READ => read(request, slots, response_cap),
        wire::OP_CLOSE => {
            slots.retain(|slot| slot.id != request.id);
            (0, Vec::new())
        }
        _ => (wire::ERR_BAD_REQUEST, Vec::new()),
    }
}

fn start(request: &wire::Header<'_>, slots: &mut Vec<HttpSlot>, next_id: &mut u32) -> (i32, Vec<u8>) {
    if slots.len() >= wire::MAX_SLOTS_PER_APP {
        return (wire::ERR_TOO_MANY, Vec::new());
    }
    let Ok(parsed) = wire::parse_start(request.payload) else {
        return (wire::ERR_BAD_REQUEST, Vec::new());
    };
    if !(parsed.url.starts_with("http://") || parsed.url.starts_with("https://")) {
        return (wire::ERR_BAD_REQUEST, Vec::new());
    }
    let mut headers: Vec<(String, String)> = Vec::new();
    if headers.try_reserve_exact(parsed.header_count).is_err() {
        return (wire::ERR_NO_MEMORY, Vec::new());
    }
    for pair in wire::HeaderPairs::new(parsed.header_bytes, parsed.header_count) {
        let Ok((name, value)) = pair else {
            return (wire::ERR_BAD_REQUEST, Vec::new());
        };
        let (Some(n), Some(v)) = (clone_str(name), clone_str(value)) else {
            return (wire::ERR_NO_MEMORY, Vec::new());
        };
        headers.push((n, v));
    }
    let (Some(method), Some(url), Some(cookie)) = (clone_str(parsed.method), clone_str(parsed.url), clone_str(parsed.cookie)) else {
        return (wire::ERR_NO_MEMORY, Vec::new());
    };
    if slots.try_reserve(1).is_err() {
        return (wire::ERR_NO_MEMORY, Vec::new());
    }
    let mut client = TcpHttpClient::new();
    client.cookie_header = cookie;
    let net = nc_net::global();
    if method.eq_ignore_ascii_case("GET") && headers.is_empty() && parsed.body.is_empty() {
        client.navigate(&url, net);
    } else {
        client.request(&method, &url, &headers, parsed.body, net);
    }
    let id = if *next_id == 0 { 1 } else { *next_id };
    *next_id = id.wrapping_add(1);
    slots.push(HttpSlot { id, client });
    (0, id.to_le_bytes().to_vec())
}

fn poll(request: &wire::Header<'_>, slots: &mut Vec<HttpSlot>, response_cap: usize) -> (i32, Vec<u8>) {
    let Some(slot) = slots.iter_mut().find(|slot| slot.id == request.id) else {
        return (wire::ERR_NO_SUCH_ID, Vec::new());
    };
    let client = &mut slot.client;
    client.tick_every = request.arg0.clamp(1, wire::MAX_TICK_EVERY);
    client.poll(nc_net::global());

    let state = state_code(client.state);
    let finished = matches!(state, wire::HttpState::Complete | wire::HttpState::Failed);
    let tls = client.tls_error.unwrap_or("");
    let tls_len = tls.len().min(120);
    let host_len = client.host.len().min(u16::MAX as usize / 2);
    let path_len = client.path.len().min(u16::MAX as usize / 2);

    let budget = response_cap.saturating_sub(wire::RESP_HEADER);
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve(wire::POLL_FIXED + tls_len + host_len + path_len + 256).is_err() {
        return (wire::ERR_NO_MEMORY, Vec::new());
    }
    out.resize(wire::POLL_FIXED, 0);
    out.extend_from_slice(&tls.as_bytes()[..tls_len]);
    out.extend_from_slice(&client.host.as_bytes()[..host_len]);
    out.extend_from_slice(&client.path.as_bytes()[..path_len]);

    let mut cookie_count = 0u16;
    let mut header_count = 0u16;
    if finished {
        let mut scratch: Vec<u8> = Vec::new();
        for (host, cookie) in client.set_cookies.iter() {
            let need = 4 + host.len() + cookie.len();
            if out.len() + need > budget {
                break;
            }
            scratch.clear();
            scratch.resize(need, 0);
            if wire::encode_pair(&mut scratch, host, cookie).is_none() {
                continue;
            }
            out.extend_from_slice(&scratch);
            cookie_count += 1;
        }
        for (name, value) in client.last_headers.iter() {
            let need = 4 + name.len() + value.len();
            if out.len() + need > budget {
                break;
            }
            scratch.clear();
            scratch.resize(need, 0);
            if wire::encode_pair(&mut scratch, name, value).is_none() {
                continue;
            }
            out.extend_from_slice(&scratch);
            header_count += 1;
        }
    }

    let fixed = wire::PollFixed {
        state,
        https: client.https,
        finished,
        tls_len: tls_len as u8,
        status_code: client.status_code,
        remote_port: client.remote_port,
        html_len: if state == wire::HttpState::Complete { client.html_body.len().min(u32::MAX as usize) as u32 } else { 0 },
        raw_len: if state == wire::HttpState::Complete { client.raw_body.len().min(u32::MAX as usize) as u32 } else { 0 },
        host_len: host_len as u16,
        path_len: path_len as u16,
        cookie_count,
        header_count,
    };
    if fixed.encode(&mut out[..wire::POLL_FIXED]).is_none() {
        return (wire::ERR_BAD_REQUEST, Vec::new());
    }
    (0, out)
}

fn read(request: &wire::Header<'_>, slots: &mut Vec<HttpSlot>, response_cap: usize) -> (i32, Vec<u8>) {
    let Some(slot) = slots.iter().find(|slot| slot.id == request.id) else {
        return (wire::ERR_NO_SUCH_ID, Vec::new());
    };
    if slot.client.state != KernelState::Complete {
        return (wire::ERR_BAD_REQUEST, Vec::new());
    }
    let source: &[u8] = match request.arg0 {
        wire::READ_HTML => slot.client.html_body.as_bytes(),
        wire::READ_RAW => slot.client.raw_body.as_slice(),
        _ => return (wire::ERR_BAD_REQUEST, Vec::new()),
    };
    let offset = request.arg1 as usize;
    if offset > source.len() {
        return (wire::ERR_BAD_REQUEST, Vec::new());
    }
    let wanted = (source.len() - offset).min(wire::MAX_CHUNK).min(response_cap.saturating_sub(wire::RESP_HEADER));
    let mut out: Vec<u8> = Vec::new();
    if out.try_reserve_exact(wanted).is_err() {
        return (wire::ERR_NO_MEMORY, Vec::new());
    }
    out.extend_from_slice(&source[offset..offset + wanted]);
    (0, out)
}
