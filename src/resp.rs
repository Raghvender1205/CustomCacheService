use bytes::{BufMut, Bytes, BytesMut};
use redis_protocol::resp2::{decode::decode_bytes_mut, types::BytesFrame};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;

use crate::engine::{Engine, ScanArgs};

#[derive(thiserror::Error, Debug)]
pub enum RespError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol: {0}")]
    Protocol(String),
}

#[derive(Debug)]
struct Cmd {
    name_upper: Bytes,
    args: Vec<Bytes>,
}

pub async fn handle_connection(mut socket: TcpStream, engine: Engine) -> Result<(), RespError> {
    let mut read_buf = BytesMut::with_capacity(8 * 1024);
    let mut write_buf = BytesMut::with_capacity(8 * 1024);

    loop {
        // read more data
        let n = socket.read_buf(&mut read_buf).await?;
        if n == 0 {
            return Ok(());
        }

        // parse as many complete frames as possible (pipelining-friendly)
        loop {
            let parsed = match decode_bytes_mut(&mut read_buf) {
                Ok(Some((frame, _amt, _consumed))) => Some(frame),
                Ok(None) => None,
                Err(e) => return Err(RespError::Protocol(format!("{:?}", e))),
            };

            let Some(frame) = parsed else { break; };

            let cmd = frame_to_cmd(frame)?;
            debug!("cmd={:?}", cmd);

            let resp = dispatch(&cmd, &engine).await;

            write_buf.clear();
            encode_resp(&resp, &mut write_buf);

            socket.write_all(&write_buf).await?;
        }
    }
}

async fn dispatch(cmd: &Cmd, engine: &Engine) -> Resp {
    let name = cmd.name_upper.as_ref();

    match name {
        b"PING" => {
            if cmd.args.is_empty() {
                Resp::Simple("PONG")
            } else {
                Resp::Bulk(Some(cmd.args[0].clone()))
            }
        }
        b"ECHO" => {
            if cmd.args.len() != 1 {
                Resp::Error("ERR wrong number of arguments for 'echo' command".into())
            } else {
                Resp::Bulk(Some(cmd.args[0].clone()))
            }
        }

        // Basic KV
        b"GET" => {
            if cmd.args.len() != 1 {
                return Resp::Error("ERR wrong number of arguments for 'get' command".into());
            }
            engine.get(cmd.args[0].clone()).await
        }
        b"SET" => {
            // Support: SET key value [PX ms]
            if cmd.args.len() < 2 {
                return Resp::Error("ERR wrong number of arguments for 'set' command".into());
            }
            let key = cmd.args[0].clone();
            let val = cmd.args[1].clone();

            let mut px_ms: Option<u64> = None;
            if cmd.args.len() >= 4 {
                if eq_icase(&cmd.args[2], b"PX") {
                    px_ms = parse_u64(&cmd.args[3]).ok();
                }
            }

            engine.set(key, val, px_ms).await
        }
        b"DEL" => {
            if cmd.args.is_empty() {
                return Resp::Error("ERR wrong number of arguments for 'del' command".into());
            }
            engine.del(cmd.args.clone()).await
        }
        b"EXISTS" => {
            if cmd.args.is_empty() {
                return Resp::Error("ERR wrong number of arguments for 'exists' command".into());
            }
            engine.exists(cmd.args.clone()).await
        }

        // TTL
        b"EXPIRE" => {
            if cmd.args.len() != 2 {
                return Resp::Error("ERR wrong number of arguments for 'expire' command".into());
            }
            let secs = match parse_u64(&cmd.args[1]) {
                Ok(v) => v,
                Err(_) => return Resp::Error("ERR value is not an integer or out of range".into()),
            };
            engine.expire_secs(cmd.args[0].clone(), secs).await
        }
        b"TTL" => {
            if cmd.args.len() != 1 {
                return Resp::Error("ERR wrong number of arguments for 'ttl' command".into());
            }
            engine.ttl(cmd.args[0].clone(), false).await
        }
        b"PTTL" => {
            if cmd.args.len() != 1 {
                return Resp::Error("ERR wrong number of arguments for 'pttl' command".into());
            }
            engine.ttl(cmd.args[0].clone(), true).await
        }

        // Redis Insight helpers
        b"HELLO" => engine.hello(cmd.args.clone()).await,
        b"CLIENT" => engine.client(cmd.args.clone()).await,
        b"COMMAND" => engine.command(cmd.args.clone()).await,
        b"INFO" => engine.info(cmd.args.clone()).await,
        b"AUTH" => {
            // No auth configured; accept any username/password combo.
            if cmd.args.is_empty() || cmd.args.len() > 2 {
                Resp::Error("ERR wrong number of arguments for 'auth' command".into())
            } else {
                Resp::Simple("OK")
            }
        }
        b"SELECT" => {
            if cmd.args.len() != 1 {
                return Resp::Error("ERR wrong number of arguments for 'select' command".into());
            }
            match parse_u64(&cmd.args[0]) {
                Ok(0) => Resp::Simple("OK"),
                Ok(_) => Resp::Error("ERR DB index is out of range".into()),
                Err(_) => Resp::Error("ERR value is not an integer or out of range".into()),
            }
        }
        b"CONFIG" => {
            if cmd.args.len() != 2 {
                return Resp::Error("ERR wrong number of arguments for 'config' command".into());
            }
            if eq_icase(&cmd.args[0], b"GET") {
                if eq_icase(&cmd.args[1], b"databases") {
                    Resp::Array(vec![
                        Resp::Bulk(Some(Bytes::from_static(b"databases"))),
                        Resp::Bulk(Some(Bytes::from_static(b"1"))),
                    ])
                } else if eq_icase(&cmd.args[1], b"requirepass") {
                    Resp::Array(vec![
                        Resp::Bulk(Some(Bytes::from_static(b"requirepass"))),
                        Resp::Bulk(Some(Bytes::from_static(b""))),
                    ])
                } else {
                    Resp::Array(vec![])
                }
            } else {
                Resp::Error("ERR unsupported CONFIG subcommand".into())
            }
        }
        b"QUIT" => Resp::Simple("OK"),
        b"DBSIZE" => engine.dbsize().await,
        b"SCAN" => {
            let args = ScanArgs::from(cmd.args.clone());
            engine.scan(args).await
        }

        _ => Resp::Error(format!(
            "ERR unknown command '{}'",
            String::from_utf8_lossy(name)
        )),
    }
}

fn frame_to_cmd(frame: BytesFrame) -> Result<Cmd, RespError> {
    let BytesFrame::Array(items) = frame else {
        return Err(RespError::Protocol("expected array frame".into()));
    };
    if items.is_empty() {
        return Err(RespError::Protocol("empty command array".into()));
    }

    let mut parts: Vec<Bytes> = Vec::with_capacity(items.len());
    for it in items {
        match it {
            BytesFrame::BulkString(b) => parts.push(b),
            BytesFrame::SimpleString(b) => parts.push(b),
            _ => return Err(RespError::Protocol("command parts must be (bulk|simple) strings".into())),
        }
    }

    let name_upper = Bytes::from(upper_ascii(parts[0].as_ref()));
    Ok(Cmd {
        name_upper,
        args: parts.into_iter().skip(1).collect(),
    })
}

fn upper_ascii(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &c in s {
        if (b'a'..=b'z').contains(&c) {
            out.push(c - 32);
        } else {
            out.push(c);
        }
    }
    out
}

fn eq_icase(a: &Bytes, b: &[u8]) -> bool {
    upper_ascii(a.as_ref()) == upper_ascii(b)
}

fn parse_u64(b: &Bytes) -> Result<u64, ()> {
    std::str::from_utf8(b.as_ref()).ok().and_then(|s| s.parse().ok()).ok_or(())
}

/* ---------------- RESP encoding ---------------- */

#[derive(Debug)]
pub enum Resp {
    Simple(&'static str),
    Error(String),
    Integer(i64),
    Bulk(Option<Bytes>),
    Array(Vec<Resp>),
}

fn encode_resp(v: &Resp, out: &mut BytesMut) {
    match v {
        Resp::Simple(s) => {
            out.put_u8(b'+');
            out.extend_from_slice(s.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        Resp::Error(e) => {
            out.put_u8(b'-');
            out.extend_from_slice(e.as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        Resp::Integer(i) => {
            out.put_u8(b':');
            out.extend_from_slice(i.to_string().as_bytes());
            out.extend_from_slice(b"\r\n");
        }
        Resp::Bulk(None) => {
            out.extend_from_slice(b"$-1\r\n");
        }
        Resp::Bulk(Some(b)) => {
            out.put_u8(b'$');
            out.extend_from_slice(b.len().to_string().as_bytes());
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(b.as_ref());
            out.extend_from_slice(b"\r\n");
        }
        Resp::Array(items) => {
            out.put_u8(b'*');
            out.extend_from_slice(items.len().to_string().as_bytes());
            out.extend_from_slice(b"\r\n");
            for it in items {
                encode_resp(it, out);
            }
        }
    }
}
