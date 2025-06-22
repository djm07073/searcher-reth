// @generated automatically by Diesel CLI.

diesel::table! {
    dex (chain_id, address, dex_type) {
        chain_id -> Integer,
        address -> Text,
        dex_type -> Integer,
    }
}

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

diesel::table! {
    token (chain_id, address, priority) {
        chain_id -> Integer,
        address -> Text,
        priority -> Integer,
    }
}

diesel::allow_tables_to_appear_in_same_query!(dex, hop, token);
