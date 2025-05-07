#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Priority {
    Beginning, // USDC or USDT, beginning token
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
}

impl From<i64> for Priority {
    fn from(value: i64) -> Self {
        match value {
            5 => Priority::VeryLow,
            4 => Priority::Low,
            3 => Priority::Medium,
            2 => Priority::High,
            1 => Priority::VeryHigh,
            0 => Priority::Beginning,
            _ => Priority::Medium,
        }
    }
}

pub type DexType = u8;
