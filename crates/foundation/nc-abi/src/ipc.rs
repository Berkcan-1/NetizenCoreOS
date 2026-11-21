//! Bounded Identity-Bound Inter-Application IPC & Scoped Intent Router (Feature 47).
//!
//! Enforces kernel-authenticated sender identity, bounded per-endpoint queues,
//! message size limits, scoped content handles (never raw arbitrary filesystem
//! paths), and explicit endpoint lifecycle/revocation.

pub const IPC_MAX_ENDPOINTS: usize = 32;
pub const IPC_QUEUE_DEPTH: usize = 8;
pub const IPC_MAX_PAYLOAD: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    TableFull,
    EndpointNotFound,
    EndpointClosed,
    QueueFull,
    QueueEmpty,
    PayloadTooLarge,
    PermissionDenied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentAction {
    OpenFile = 1,
    ShareFile = 2,
    OpenUrl = 3,
    Custom = 4,
}

#[derive(Clone, Copy)]
pub struct IpcMessage {
    pub sender_slot: u32,
    pub sender_id_len: u8,
    pub sender_id: [u8; 32],
    pub action: u8,
    pub scoped_handle: u32,
    pub payload_len: u16,
    pub payload: [u8; IPC_MAX_PAYLOAD],
}

impl IpcMessage {
    pub const EMPTY: Self = Self {
        sender_slot: 0,
        sender_id_len: 0,
        sender_id: [0; 32],
        action: 0,
        scoped_handle: 0,
        payload_len: 0,
        payload: [0; IPC_MAX_PAYLOAD],
    };
}

#[derive(Clone, Copy)]
struct Endpoint {
    in_use: bool,
    closed: bool,
    endpoint_id: u32,
    owner_slot: u32,
    head: usize,
    count: usize,
    queue: [IpcMessage; IPC_QUEUE_DEPTH],
}

impl Endpoint {
    const EMPTY: Self = Self {
        in_use: false,
        closed: false,
        endpoint_id: 0,
        owner_slot: 0,
        head: 0,
        count: 0,
        queue: [IpcMessage::EMPTY; IPC_QUEUE_DEPTH],
    };
}

pub struct IpcBroker {
    endpoints: [Endpoint; IPC_MAX_ENDPOINTS],
    next_id: u32,
}

impl IpcBroker {
    pub const fn new() -> Self {
        Self {
            endpoints: [Endpoint::EMPTY; IPC_MAX_ENDPOINTS],
            next_id: 1,
        }
    }

    pub fn create_endpoint(&mut self, owner_slot: u32) -> Result<u32, IpcError> {
        let ep = self
            .endpoints
            .iter_mut()
            .find(|e| !e.in_use)
            .ok_or(IpcError::TableFull)?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        *ep = Endpoint::EMPTY;
        ep.in_use = true;
        ep.endpoint_id = id;
        ep.owner_slot = owner_slot;
        Ok(id)
    }

    /// Send an identity-bound message after SPARK `Authorize_IPC` succeeds.
    #[allow(clippy::too_many_arguments)]
    pub fn send(
        &mut self,
        endpoint_id: u32,
        authorized_by_spark: bool,
        sender_slot: u32,
        sender_app_id: &str,
        action: IntentAction,
        scoped_handle: u32,
        payload: &[u8],
    ) -> Result<(), IpcError> {
        if !authorized_by_spark {
            return Err(IpcError::PermissionDenied);
        }
        if payload.len() > IPC_MAX_PAYLOAD {
            return Err(IpcError::PayloadTooLarge);
        }
        let ep = self
            .endpoints
            .iter_mut()
            .find(|e| e.in_use && e.endpoint_id == endpoint_id)
            .ok_or(IpcError::EndpointNotFound)?;
        if ep.closed {
            return Err(IpcError::EndpointClosed);
        }
        if ep.count >= IPC_QUEUE_DEPTH {
            return Err(IpcError::QueueFull);
        }
        let tail = (ep.head + ep.count) % IPC_QUEUE_DEPTH;
        let mut msg = IpcMessage::EMPTY;
        msg.sender_slot = sender_slot;
        let id_b = sender_app_id.as_bytes();
        let id_len = id_b.len().min(32);
        msg.sender_id_len = id_len as u8;
        msg.sender_id[..id_len].copy_from_slice(&id_b[..id_len]);
        msg.action = action as u8;
        msg.scoped_handle = scoped_handle;
        msg.payload_len = payload.len() as u16;
        msg.payload[..payload.len()].copy_from_slice(payload);
        ep.queue[tail] = msg;
        ep.count += 1;
        Ok(())
    }

    pub fn recv(&mut self, caller_slot: u32, endpoint_id: u32) -> Result<IpcMessage, IpcError> {
        let ep = self
            .endpoints
            .iter_mut()
            .find(|e| e.in_use && e.endpoint_id == endpoint_id)
            .ok_or(IpcError::EndpointNotFound)?;
        if ep.owner_slot != caller_slot {
            return Err(IpcError::PermissionDenied);
        }
        if ep.count == 0 {
            return Err(if ep.closed {
                IpcError::EndpointClosed
            } else {
                IpcError::QueueEmpty
            });
        }
        let msg = ep.queue[ep.head];
        ep.head = (ep.head + 1) % IPC_QUEUE_DEPTH;
        ep.count -= 1;
        Ok(msg)
    }

    pub fn close_endpoint(&mut self, caller_slot: u32, endpoint_id: u32) -> Result<(), IpcError> {
        let ep = self
            .endpoints
            .iter_mut()
            .find(|e| e.in_use && e.endpoint_id == endpoint_id)
            .ok_or(IpcError::EndpointNotFound)?;
        if ep.owner_slot != caller_slot {
            return Err(IpcError::PermissionDenied);
        }
        ep.closed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_broker_enforces_spark_auth_identity_bounds_and_lifecycle() {
        let mut broker = IpcBroker::new();
        let ep = broker.create_endpoint(5).unwrap();

        // Denied by SPARK -> rejected.
        assert_eq!(
            broker.send(ep, false, 2, "nc.files", IntentAction::OpenFile, 99, b"doc"),
            Err(IpcError::PermissionDenied)
        );

        // Authorized send succeeds and preserves authenticated sender identity & scoped handle.
        broker
            .send(ep, true, 2, "nc.files", IntentAction::OpenFile, 99, b"doc")
            .unwrap();

        // Another task (slot 7) cannot read endpoint owned by slot 5.
        assert_eq!(broker.recv(7, ep).err(), Some(IpcError::PermissionDenied));

        let msg = broker.recv(5, ep).unwrap();
        assert_eq!(msg.sender_slot, 2);
        assert_eq!(&msg.sender_id[..msg.sender_id_len as usize], b"nc.files");
        assert_eq!(msg.scoped_handle, 99);
        assert_eq!(&msg.payload[..msg.payload_len as usize], b"doc");

        // Closing endpoint rejects further sends.
        broker.close_endpoint(5, ep).unwrap();
        assert_eq!(
            broker.send(ep, true, 2, "nc.files", IntentAction::OpenFile, 100, b"x"),
            Err(IpcError::EndpointClosed)
        );
    }
}
