//! Bounded local transport shared by the trusted UI and controller.
use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};
use zeroize::Zeroizing;

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub data: Value,
}

impl Drop for Request {
    fn drop(&mut self) {
        // Also scrub on error returns; never log request payloads.
        scrub(&mut self.data);
    }
}

pub fn rpc(runtime: &Path, request: Request) -> Result<Value> {
    let mut socket = UnixStream::connect(runtime.join("control.sock"))?;
    socket.set_read_timeout(Some(Duration::from_secs(90)))?;
    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
    send_frame(&mut socket, &Zeroizing::new(serde_json::to_vec(&request)?))?;
    let mut reply: Value = serde_json::from_slice(&receive_frame(&mut socket)?)?;
    if let Some(e) = reply.get("error").and_then(Value::as_str) {
        return Err(anyhow!(e.to_string()));
    }
    reply
        .get_mut("result")
        .map(Value::take)
        .context("Invalid controller response")
}
pub fn send_frame(socket: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= MAX_RPC_FRAME_BYTES, "Request too large");
    socket.write_all(&(bytes.len() as u32).to_be_bytes())?;
    socket.write_all(bytes)?;
    Ok(())
}
const MAX_RPC_FRAME_BYTES: usize = 64 * 1024;

pub fn receive_frame(socket: &mut UnixStream) -> Result<Zeroizing<Vec<u8>>> {
    let mut header = [0; 4];
    socket.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    ensure!(length <= MAX_RPC_FRAME_BYTES, "Request too large");
    let mut payload = Zeroizing::new(vec![0; length]);
    socket.read_exact(&mut payload)?;
    Ok(payload)
}

fn scrub(v: &mut Value) {
    use zeroize::Zeroize;
    match v {
        Value::String(s) => s.zeroize(),
        Value::Array(v) => v.iter_mut().for_each(scrub),
        Value::Object(m) => m.values_mut().for_each(scrub),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversize_rpc_before_allocating() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        sender.write_all(&u32::MAX.to_be_bytes()).unwrap();
        assert!(receive_frame(&mut receiver).is_err());
    }

    #[test]
    fn rejects_oversize_send_without_writing_a_header() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        assert!(send_frame(&mut sender, &vec![0; MAX_RPC_FRAME_BYTES + 1]).is_err());
        receiver.set_nonblocking(true).unwrap();
        assert_eq!(
            receiver.read(&mut [0; 4]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn rejects_truncated_payload() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        sender.write_all(&10u32.to_be_bytes()).unwrap();
        sender.write_all(b"short").unwrap();
        drop(sender);
        assert!(receive_frame(&mut receiver).is_err());
    }

    #[test]
    fn round_trips_framed_payload() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        send_frame(&mut sender, b"synthetic payload").unwrap();
        assert_eq!(
            &**receive_frame(&mut receiver).unwrap(),
            b"synthetic payload"
        );
    }

    #[test]
    fn scrubs_nested_secret_values() {
        let mut data = serde_json::json!({"secret": "password", "nested": [{"pin": "123456"}]});
        scrub(&mut data);
        assert_eq!(
            data,
            serde_json::json!({"secret": "", "nested": [{"pin": ""}]})
        );
    }
}
