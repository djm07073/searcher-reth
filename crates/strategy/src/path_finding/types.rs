use alloy_sol_types::sol;

sol! {
    #[derive(Debug)]
    struct Hop {
        uint8 dexType;
        address router;
        address quoter;
        address srcToken;
        address dstToken;
        bytes metadata; // Additional metadata for the hop ex. Balancer pool id or Uniswap v3 fee
    }

    #[derive(Debug)]
    struct QuoterHop {
        uint8 dexType;
        address quoter;
        address srcToken;
        address dstToken;
        bytes metadata; // Additional metadata for the hop ex. Balancer pool id or Uniswap v3 fee
    }

    // Calculate the profit for a given route
    #[derive(Debug)]
    function getProfit(uint256 amount, QuoterHop[] memory route) view external returns (uint256 profit);

    // basic elements of the path finding contract
    #[derive(Debug)]
    struct RouterHop {
        uint8 dexType;
        address router;
        address srcToken;
        address dstToken;
        bytes metadata; // Additional metadata for the hop ex. Balancer pool id or Uniswap v3 fee
    }


    // Execute a route and return the profit
    #[derive(Debug)]
    function execute(
        uint256[] memory amounts,
        RouterHop[][] memory routes
    ) external returns (uint256 profit);
}

impl From<Hop> for RouterHop {
    fn from(hop: Hop) -> Self {
        Self {
            dexType: hop.dexType,
            router: hop.router,
            srcToken: hop.srcToken,
            dstToken: hop.dstToken,
            metadata: hop.metadata,
        }
    }
}

impl From<Hop> for QuoterHop {
    fn from(hop: Hop) -> Self {
        Self {
            dexType: hop.dexType,
            quoter: hop.quoter,
            srcToken: hop.srcToken,
            dstToken: hop.dstToken,
            metadata: hop.metadata,
        }
    }
}
