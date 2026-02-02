use std::io::Cursor;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use super::cache::Cache;
use log::{info, error};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::error::Error;

#[derive(Debug)]
pub enum Command {
    Set { key: String, value: String },
    Get { key: String },
    Delete { key: String },
    Expire { key: String, seconds: u64 },
    Incr { key: String },
    Decr { key: String },
    Keys { pattern: String },
}

impl Command {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let mut cursor = Cursor::new(bytes);
        let command_type = ReadBytesExt::read_u8(&mut cursor)?;

        match command_type {
            0x01 => {
                let key = read_string(&mut cursor)?;
                let value = read_string(&mut cursor)?;
                Ok(Command::Set { key, value })
            },
            0x02 => {
                let key = read_string(&mut cursor)?;
                Ok(Command::Get { key })
            },
            0x03 => {
                let key = read_string(&mut cursor)?;
                Ok(Command::Delete { key })
            },
            0x04 => {
                let key = read_string(&mut cursor)?;
                let seconds = ReadBytesExt::read_u64::<LittleEndian>(&mut cursor)?;
                Ok(Command::Expire { key, seconds })
            },
            0x05 => {
                let key = read_string(&mut cursor)?;
                Ok(Command::Incr { key })
            },
            0x06 => {
                let key = read_string(&mut cursor)?;
                Ok(Command::Decr { key })
            },
            0x07 => {
                let pattern = read_string(&mut cursor)?;
                Ok(Command::Keys { pattern })
            },
            _ => Err("Unknown command type".into()),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        match self {
            Command::Set { key, value } => {
                bytes.push(0x01);
                write_string(&mut bytes, key);
                write_string(&mut bytes, value);
            },
            Command::Get { key } => {
                bytes.push(0x02);
                write_string(&mut bytes, key);
            },
            Command::Delete { key } => {
                bytes.push(0x03);
                write_string(&mut bytes, key);
            },
            Command::Expire { key, seconds } => {
                bytes.push(0x04);
                write_string(&mut bytes, key);
                WriteBytesExt::write_u64::<LittleEndian>(&mut bytes, *seconds).unwrap();
            },
            Command::Incr { key } => {
                bytes.push(0x05);
                write_string(&mut bytes, key);
            },
            Command::Decr { key } => {
                bytes.push(0x06);
                write_string(&mut bytes, key);
            },
            Command::Keys { pattern } => {
                bytes.push(0x07);
                write_string(&mut bytes, pattern);
            },
        }
        bytes
    }
}

fn read_string(cursor: &mut Cursor<&[u8]>) -> Result<String, Box<dyn Error>> {
    let length = ReadBytesExt::read_u8(cursor)? as usize;
    let mut buffer = vec![0; length];
    std::io::Read::read_exact(cursor, &mut buffer)?;
    String::from_utf8(buffer).map_err(|e| e.into())
}

fn write_string(buffer: &mut Vec<u8>, s: &str) {
    WriteBytesExt::write_u8(buffer, s.len() as u8).unwrap();
    buffer.extend_from_slice(s.as_bytes());
}

pub async fn handle_connection(mut socket: TcpStream, cache: Arc<Cache>) -> Result<(), Box<dyn Error>> {
    let mut buffer = vec![0; 1024];

    loop {
        let n = socket.read(&mut buffer).await?;

        if n == 0 {
            info!("Connection closed");
            return Ok(());
        }

        let command = tokio::task::block_in_place(|| Command::from_bytes(&buffer[..n]))?;
        info!("Received command: {:?}", command);
        if let Some(response) = cache.handle_command(command).await {
            if let Err(e) = socket.write_all(response.as_bytes()).await {
                error!("Failed to write to socket: {}", e);
                return Err(e.into());
            }
        }
    }
}