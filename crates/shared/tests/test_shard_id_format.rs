#[cfg(test)]
mod tests {
    use shared::colony_model::Shard;

    #[test]
    fn test_shard_to_id_basic() {
        let shard = Shard { x: 0, y: 0, width: 500, height: 500 };
        assert_eq!(shard.to_id(), "0_0_500_500");
    }

    #[test]
    fn test_shard_to_id_positive_coordinates() {
        let shard = Shard { x: 500, y: 0, width: 500, height: 500 };
        assert_eq!(shard.to_id(), "500_0_500_500");
    }

    #[test]
    fn test_shard_to_id_negative_coordinates() {
        let shard = Shard { x: -100, y: -200, width: 300, height: 400 };
        assert_eq!(shard.to_id(), "-100_-200_300_400");
    }

    #[test]
    fn test_shard_to_id_large_numbers() {
        let shard = Shard { x: 1000000, y: 2000000, width: 3000000, height: 4000000 };
        assert_eq!(shard.to_id(), "1000000_2000000_3000000_4000000");
    }

    #[test]
    fn test_shard_from_id_basic() {
        let id = "0_0_500_500";
        let shard = Shard::from_id(id).unwrap();
        assert_eq!(shard.x, 0);
        assert_eq!(shard.y, 0);
        assert_eq!(shard.width, 500);
        assert_eq!(shard.height, 500);
    }

    #[test]
    fn test_shard_from_id_positive_coordinates() {
        let id = "500_0_500_500";
        let shard = Shard::from_id(id).unwrap();
        assert_eq!(shard.x, 500);
        assert_eq!(shard.y, 0);
        assert_eq!(shard.width, 500);
        assert_eq!(shard.height, 500);
    }

    #[test]
    fn test_shard_from_id_negative_coordinates() {
        let id = "-100_-200_300_400";
        let shard = Shard::from_id(id).unwrap();
        assert_eq!(shard.x, -100);
        assert_eq!(shard.y, -200);
        assert_eq!(shard.width, 300);
        assert_eq!(shard.height, 400);
    }

    #[test]
    fn test_shard_from_id_large_numbers() {
        let id = "1000000_2000000_3000000_4000000";
        let shard = Shard::from_id(id).unwrap();
        assert_eq!(shard.x, 1000000);
        assert_eq!(shard.y, 2000000);
        assert_eq!(shard.width, 3000000);
        assert_eq!(shard.height, 4000000);
    }

    #[test]
    fn test_shard_round_trip() {
        let original = Shard { x: 123, y: -456, width: 789, height: -1011 };
        let id = original.to_id();
        let parsed = Shard::from_id(&id).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_shard_round_trip_zero() {
        let original = Shard { x: 0, y: 0, width: 0, height: 0 };
        let id = original.to_id();
        let parsed = Shard::from_id(&id).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_shard_from_id_invalid_too_few_parts() {
        let id = "0_0_500";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 4 parts"));
    }

    #[test]
    fn test_shard_from_id_invalid_too_many_parts() {
        let id = "0_0_500_500_600";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 4 parts"));
    }

    #[test]
    fn test_shard_from_id_invalid_non_integer_x() {
        let id = "abc_0_500_500";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid x coordinate"));
    }

    #[test]
    fn test_shard_from_id_invalid_non_integer_y() {
        let id = "0_xyz_500_500";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid y coordinate"));
    }

    #[test]
    fn test_shard_from_id_invalid_non_integer_width() {
        let id = "0_0_def_500";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid width"));
    }

    #[test]
    fn test_shard_from_id_invalid_non_integer_height() {
        let id = "0_0_500_ghi";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid height"));
    }

    #[test]
    fn test_shard_from_id_invalid_overflow() {
        let id = "999999999999999999999_0_500_500";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid x coordinate"));
    }

    #[test]
    fn test_shard_from_id_empty_string() {
        let id = "";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 4 parts"));
    }

    #[test]
    fn test_shard_from_id_no_underscores() {
        let id = "00500500";
        let result = Shard::from_id(id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 4 parts"));
    }
}
