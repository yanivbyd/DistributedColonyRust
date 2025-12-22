#[test]
fn test_generate_colony_instance_id() {
    let id = shared::utils::generate_colony_instance_id();
    assert_eq!(id.len(), 3);
    assert!(id.chars().all(|c| c.is_ascii_lowercase()));
}

