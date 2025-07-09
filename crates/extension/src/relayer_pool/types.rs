use alloy_sol_types::sol;

sol! {
    event Profit(address indexed token, uint256 indexed profit, uint256 amountIn, uint256 amountOut);
}
