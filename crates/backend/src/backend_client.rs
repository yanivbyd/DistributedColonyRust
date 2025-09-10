use shared::be_api::{BackendRequest, BackendResponse, UpdatedShardContentsRequest};
use shared::cluster_topology::HostInfo;
use shared::backend_communication::send_request_and_receive_response_async;
use tokio::net::TcpStream;

pub async fn send_updated_shard_contents_to_host_async(
    host: &HostInfo,
    req: &UpdatedShardContentsRequest,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = host.to_address();
    let mut stream = TcpStream::connect(&addr).await?;
    
    let request = BackendRequest::UpdatedShardContents(req.clone());
    let response: BackendResponse = send_request_and_receive_response_async(&mut stream, &request).await?;
    
    match response {
        BackendResponse::UpdatedShardContents(_) => Ok(()),
        _ => Err("Unexpected response type".into()),
    }
}


