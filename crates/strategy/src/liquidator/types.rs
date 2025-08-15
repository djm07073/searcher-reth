use alloy_sol_types::sol;

sol! {
    #[derive(Debug)]
    struct LiquidatorTodoAction {
        uint256 amount;
        address token;
        address to;
    }
}