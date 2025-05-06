use alloy_primitives::{ address, Address };
use alloy_sol_types::sol;

pub(crate) const DEPLOYED_ADDRESS: Address = address!("0000000000000000000000000000000000012345");

sol! {
    struct Hop {
        uint8 dexType;
        address dex;
        address srcToken;
        address dstToken;
    }

    struct RoutePath {
        Hop[] hops;
    }

    struct Profit {
        uint256 amount;
    }
}
