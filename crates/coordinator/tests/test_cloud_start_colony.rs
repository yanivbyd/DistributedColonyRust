#![cfg(all(test, feature = "cloud"))]

use coordinator::cloud_start::cloud_start_colony;
use shared::cluster_topology::NodeAddress;
use shared::ssm::{set_mock_provider, SsmProvider};
use shared::be_api::{BackendRequest, BackendResponse, InitColonyRequest, InitColonyResponse, InitColonyShardRequest, InitColonyShardResponse, GetColonyInfoResponse};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

async fn run_mock_backend(port: u16, init_counter: Arc<AtomicUsize>, last_init: Arc<Mutex<Option<InitColonyRequest>>>) {
    use tokio::net::TcpListener;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    let listener = TcpListener::bind(("127.0.0.1", port)).await.expect("bind backend");
    loop {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let init_counter = init_counter.clone();
        let last_init = last_init.clone();
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            if socket.read_exact(&mut len_buf).await.is_err() { return; }
            let len = u32::from_be_bytes(len_buf) as usize;
            let mut buf = vec![0u8; len];
            if socket.read_exact(&mut buf).await.is_err() { return; }

            // Try BackendRequest
            if let Ok(req) = bincode::deserialize::<BackendRequest>(&buf) {
                match req {
                    BackendRequest::Ping => {
                        let resp = BackendResponse::Ping;
                        let enc = bincode::serialize(&resp).unwrap();
                        let len = (enc.len() as u32).to_be_bytes();
                        let _ = socket.write_all(&len).await;
                        let _ = socket.write_all(&enc).await;
                    }
                    BackendRequest::GetColonyInfo(_) => {
                        let resp = BackendResponse::GetColonyInfo(GetColonyInfoResponse::ColonyNotInitialized);
                        let enc = bincode::serialize(&resp).unwrap();
                        let len = (enc.len() as u32).to_be_bytes();
                        let _ = socket.write_all(&len).await;
                        let _ = socket.write_all(&enc).await;
                    }
                    BackendRequest::InitColony(init) => {
                        init_counter.fetch_add(1, Ordering::SeqCst);
                        *last_init.lock().unwrap() = Some(init);
                        let resp = BackendResponse::InitColony(InitColonyResponse::Ok);
                        let enc = bincode::serialize(&resp).unwrap();
                        let len = (enc.len() as u32).to_be_bytes();
                        let _ = socket.write_all(&len).await;
                        let _ = socket.write_all(&enc).await;
                    }
                    BackendRequest::InitColonyShard(InitColonyShardRequest { .. }) => {
                        let resp = BackendResponse::InitColonyShard(InitColonyShardResponse::Ok);
                        let enc = bincode::serialize(&resp).unwrap();
                        let len = (enc.len() as u32).to_be_bytes();
                        let _ = socket.write_all(&len).await;
                        let _ = socket.write_all(&enc).await;
                    }
                    _ => {}
                }
            }
        });
    }
}

#[tokio::test]
async fn test_cloud_start_colony_initializes_two_backends() {
    // Arrange: two mock backends that respond to Ping and init requests
    let port1 = 18082u16;
    let port2 = 18083u16;
    // Set mock SSM provider
    struct Mock { p1: u16, p2: u16 }
    impl SsmProvider for Mock {
        fn discover_coordinator(&self) -> Option<NodeAddress> { None }
        fn discover_backends(&self) -> Vec<NodeAddress> {
            vec![
                NodeAddress::new("127.0.0.1".to_string(), self.p1),
                NodeAddress::new("127.0.0.1".to_string(), self.p2),
            ]
        }
    }
    set_mock_provider(Some(std::sync::Arc::new(Mock { p1: port1, p2: port2 })));

    let init_counter1 = Arc::new(AtomicUsize::new(0));
    let init_counter2 = Arc::new(AtomicUsize::new(0));
    let last_init1: Arc<Mutex<Option<InitColonyRequest>>> = Arc::new(Mutex::new(None));
    let last_init2: Arc<Mutex<Option<InitColonyRequest>>> = Arc::new(Mutex::new(None));

    tokio::spawn(run_mock_backend(port1, init_counter1.clone(), last_init1.clone()));
    tokio::spawn(run_mock_backend(port2, init_counter2.clone(), last_init2.clone()));

    // Act: run cloud_start_colony (should discover, ping, create shard map, init colony)
    // Give backends a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    cloud_start_colony().await;

    // Assert: each backend received an InitColony
    assert!(init_counter1.load(Ordering::SeqCst) >= 1);
    assert!(init_counter2.load(Ordering::SeqCst) >= 1);

    // Verify parameters of InitColony
    let init1 = last_init1.lock().unwrap().take().expect("backend1 init colony received");
    let init2 = last_init2.lock().unwrap().take().expect("backend2 init colony received");
    assert_eq!(init1.width, init2.width);
    assert_eq!(init1.height, init2.height);

    let expected_width = shared::cluster_topology::ClusterTopology::get_width_in_shards() * shared::cluster_topology::ClusterTopology::get_shard_width();
    let expected_height = shared::cluster_topology::ClusterTopology::get_height_in_shards() * shared::cluster_topology::ClusterTopology::get_shard_height();
    assert_eq!(init1.width, expected_width);
    assert_eq!(init1.height, expected_height);

    // Colony rules should match the coordinator defaults (compare fields)
    let rules = init1.colony_life_rules;
    let expected = coordinator::init_colony::COLONY_LIFE_INITIAL_RULES;
    assert_eq!(rules.health_cost_per_size_unit, expected.health_cost_per_size_unit);
    assert_eq!(rules.eat_capacity_per_size_unit, expected.eat_capacity_per_size_unit);
    assert_eq!(rules.health_cost_if_can_kill, expected.health_cost_if_can_kill);
    assert_eq!(rules.health_cost_if_can_move, expected.health_cost_if_can_move);
    assert_eq!(rules.mutation_chance, expected.mutation_chance);
    assert_eq!(rules.random_death_chance, expected.random_death_chance);
}


