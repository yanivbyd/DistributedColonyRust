use std::net::TcpStream;
use std::io::{Read, Write};
use bincode;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn send_request<T: serde::Serialize>(stream: &mut TcpStream, request: &T) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = bincode::serialize(request)?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)?;
    Ok(())
}

pub fn receive_response<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Result<T, Box<dyn std::error::Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf)?;
    let response = bincode::deserialize(&buf)?;
    Ok(response)
}

pub async fn send_request_async<T: serde::Serialize>(stream: &mut TokioTcpStream, request: &T) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let encoded = bincode::serialize(request)?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&encoded).await?;
    Ok(())
}

pub async fn receive_response_async<T: serde::de::DeserializeOwned>(stream: &mut TokioTcpStream) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).await?;
    let response = bincode::deserialize(&buf)?;
    Ok(response)
}

pub async fn send_request_and_receive_response_async<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
    stream: &mut TokioTcpStream, 
    request: &Req
) -> Result<Resp, Box<dyn std::error::Error + Send + Sync>> {
    send_request_async(stream, request).await?;
    receive_response_async(stream).await
}
