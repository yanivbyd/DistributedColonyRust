use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, GetShardImageResponse, InitColonyShardResponse, InitColonyRequest, GetShardImageRequest, InitColonyShardRequest, InitColonyResponse, GetColonyInfoRequest, GetColonyInfoResponse, UpdatedShardContentsRequest, UpdatedShardContentsResponse};
use bincode;
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error};

mod colony;
mod ticker;
mod colony_shard;
mod shard_utils;
mod colony_events;
mod topography;

use crate::colony::Colony;
use crate::shard_utils::ShardUtils;

// Debug logging macro that does nothing by default
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

fn call_label(response: &BackendResponse) -> &'static str {
    match response {
        BackendResponse::Ping => "Ping",
        BackendResponse::InitColony(_) => "InitColony",
        BackendResponse::GetShardImage(_) => "GetShardImage",
        BackendResponse::InitColonyShard(_) => "InitColonyShard",
        BackendResponse::GetColonyInfo(_) => "GetColonyInfo",
        BackendResponse::UpdatedShardContents(_) => todo!(),
    }
}

async fn send_response(framed: &mut FramedStream, response: BackendResponse) {
    let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
    let label = call_label(&response);
    if let Err(e) = framed.send(encoded.into()).await {
        log_error!("[BE] Failed to send {} response: {}", label, e);
    } else {
        log_debug!("[BE] Sent {} response", label);
    }
}

async fn handle_client(socket: TcpStream) {
    log_debug!("[BE] handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log_debug!("[BE] handle_client: received bytes");
        let response = match bincode::deserialize::<BackendRequest>(&bytes) {
            Ok(BackendRequest::Ping) => handle_ping().await,
            Ok(BackendRequest::InitColony(req)) => handle_init_colony(req).await,
            Ok(BackendRequest::GetShardImage(req)) => handle_get_shard_image(req).await,
            Ok(BackendRequest::InitColonyShard(req)) => handle_init_colony_shard(req).await,
            Ok(BackendRequest::GetColonyInfo(req)) => handle_get_colony_info(req).await,
            Ok(BackendRequest::UpdatedShardContents(req)) => handle_updated_shard_contents(req).await,
            Err(e) => {
                log_error!("[BE] Failed to deserialize BackendRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
    log_debug!("[BE] handle_client: connection closed");
}

async fn handle_ping() -> BackendResponse {
    BackendResponse::Ping
}

async fn handle_init_colony(req: InitColonyRequest) -> BackendResponse {
    if Colony::is_initialized() {
        BackendResponse::InitColony(InitColonyResponse::ColonyAlreadyInitialized)
    } else {
        Colony::init(&req);
        BackendResponse::InitColony(InitColonyResponse::Ok)
    }
}

async fn handle_init_colony_shard(req: InitColonyShardRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized)
    } else if Colony::instance().has_shard(req.shard) {
        BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized)
    } else if !Colony::instance().is_valid_shard_dimensions(&req.shard) {
        BackendResponse::InitColonyShard(InitColonyShardResponse::InvalidShardDimensions)
    } else {
        Colony::instance().add_shard(ShardUtils::new_colony_shard(&req.shard, &req.colony_life_info));
        BackendResponse::InitColonyShard(InitColonyShardResponse::Ok)
    }
}

async fn handle_get_shard_image(req: GetShardImageRequest) -> BackendResponse {
    log_debug!("[BE] GetShardImage request: shard=({},{},{},{})", req.shard.x, req.shard.y, req.shard.width, req.shard.height);
    let colony = Colony::instance();
    if let Some(shard) = &colony.get_colony_shard(&req.shard) {
        match ShardUtils::get_shard_image(shard, &req.shard) {
            Some(image) => BackendResponse::GetShardImage(GetShardImageResponse::Image { image }),
            None => BackendResponse::GetShardImage(GetShardImageResponse::ShardNotAvailable),
        }
    } else {
        BackendResponse::GetShardImage(GetShardImageResponse::ShardNotAvailable)
    }
}

async fn handle_get_colony_info(_req: GetColonyInfoRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::ColonyNotInitialized)
    } else {
        let colony = Colony::instance();
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::Ok {
            width: colony._width,
            height: colony._height,
            shards: colony.shards.iter().map(|cs| cs.shard).collect(),
        })
    }
}

async fn handle_updated_shard_contents(req: UpdatedShardContentsRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        return BackendResponse::UpdatedShardContents(UpdatedShardContentsResponse {});
    }
    let mut colony = Colony::instance();
    for shard in colony.shards.iter_mut() {
        ShardUtils::updated_shard_contents(shard, &req);
    }
    BackendResponse::UpdatedShardContents(UpdatedShardContentsResponse {})
}

#[tokio::main]
async fn main() {
    init_logging("output/logs/be.log");
    log_startup("BE");
    set_panic_hook();
    shared::metrics::start_metrics_endpoint();
    ticker::start_ticker();
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    log_debug!("[BE] Listening on {}", addr);

    loop {
        log_debug!("[BE] Waiting for connection...");
        match listener.accept().await {
            Ok((socket, _)) => {
                log_debug!("[BE] Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("[BE] Connection failed: {}", e),
        }
    }
} 