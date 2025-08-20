#[macro_export]
macro_rules! install_strategy {
    (
        $builder:expr,
        $config:expr,
        $wallet:expr,
        $signal_manager:expr,
        $exex_id:expr,
        $strategy_type:ty
    ) => {{
        use searcher_reth_extension::strategy::core::strategy::Strategy;

        let cfg = $config.read().unwrap().get_strategy($exex_id).unwrap();
        let strategy = <$strategy_type>::new(cfg);
        let searcher_exex = searcher_reth_extension::exex::SearcherExEx::new(
            $wallet.clone(),
            $signal_manager.subscribe(),
        );
        $builder.install_exex($exex_id, move |ctx| searcher_exex.run(ctx, strategy))
    }};
}
