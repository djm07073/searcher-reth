// impl of WithLaunchContext
pub trait SearcherExtension {
    fn install_searcher_extension(self, args: SearcherExtensionArgs) -> Self;
}

impl<Builder> SearcherExtension for WithLaunchContext<Builder> {
    fn install_searcher_extension(self, args: SearcherExtensionArgs) -> Self {
        // TODO: fetch the bytecode and route paths from database
        // if there are not, make panic
        let bytecode = todo!();
        let route_paths = todo!();
        self.transform(|builder, args| {
            builder
                .extend_rpc_modules(move |ctx| { SearcherRpc::init(ctx) })
                .install_exex("ShadowExEx", move |ctx| {
                    SearcherExEx::init(ctx, bytecode, route_paths)
                })
        })
    }
}
