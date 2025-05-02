#[derive(sqlx::Type, Debug)]
#[sqlx(type_name = "priority", rename_all = "lowercase")]
pub enum Priority {
    VeryLow,
    Low,
    Medium,
    High,
    VeryHigh,
}

impl From<i64> for Priority {
    fn from(value: i64) -> Self {
        match value {
            0 => Priority::VeryLow,
            1 => Priority::Low,
            2 => Priority::Medium,
            3 => Priority::High,
            4 => Priority::VeryHigh,
            _ => Priority::Medium,
        }
    }
}
