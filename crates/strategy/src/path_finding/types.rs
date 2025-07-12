use alloy_sol_types::sol;

sol! {
    // basic elements of the path finding contract
    #[derive(Debug)]
    struct Hop {
        uint8 dexType;
        address dex;
        address srcToken;
        address dstToken;
        bytes metadata; // Additional metadata for the hop ex. Balancer
    }

    // Calculate the profit for a given route
    #[derive(Debug)]
    function getProfit(uint256 amount, Hop[] memory route) view external returns (uint256 profit);

    // Execute a route and return the profit
    #[derive(Debug)]
    function execute(
        uint256[] memory amounts,
        Hop[][] memory routes
    ) external returns (uint256 profit);
}
