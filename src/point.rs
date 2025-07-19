#[derive(Debug)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_new() {
        let point = Point::new(1.5, 2.5);
        assert_eq!(point.x, 1.5);
        assert_eq!(point.y, 2.5);
    }

    #[test]
    fn test_point_debug() {
        let point = Point::new(0.0, 0.0);
        let debug_str = format!("{:?}", point);
        assert!(debug_str.contains("Point"));
    }
}
