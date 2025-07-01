// @generated automatically by Diesel CLI.

diesel::table! {
    hop (chain_id, address, dex_type, src_token, dst_token) {
        chain_id -> Integer,
        address -> Text,
        dex_type -> Integer,
        src_token -> Text,
        dst_token -> Text,
        hop_type -> Integer,
        metadata -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(hop,);
