CREATE TABLE contract (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    bytecode TEXT NOT NULL,
    metadata TEXT NOT NULL,
    PRIMARY KEY (chain_id, address)
);

CREATE TABLE token (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    priority INTEGER NOT NULL,
    PRIMARY KEY (chain_id, address, priority)
);

CREATE TABLE dex (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    dex_type INTEGER NOT NULL,
    PRIMARY KEY (chain_id, address, dex_type)
);

CREATE TABLE hop (
    chain_id INTEGER NOT NULL,
    address TEXT NOT NULL,
    dex_type INTEGER NOT NULL,
    src_token TEXT NOT NULL,
    dst_token TEXT NOT NULL,
    hop_type INTEGER NOT NULL,
    metadata TEXT NOT NULL,
    PRIMARY KEY (chain_id, address, dex_type, src_token, dst_token)
);