use alloy_sol_types::sol;

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

sol! {
    struct Hop {
        uint8 dexType;
        address dex;
        address srcToken;
        address dstToken;
    }
}
pub type RoutePath = Vec<Hop>; // dex type, dex, src token ,dst token
