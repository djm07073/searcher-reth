# Searcher Reth

This repository contains a set of crates that build a searcher on top of the
[Reth](https://github.com/paradigmxyz/reth) client.  Each crate has a focused
responsibility which is summarized below.

## Crates

| Crate | Role |
|-------|------|
| **bin/searcher-reth** | Binary that launches Reth with the searcher extension installed. |
| **crates/manager** | Configuration loading and signal handling utilities. |
| **crates/strategy** | Strategy implementations and core traits. |
| **crates/extension** | Glue code that runs inside Reth's ExEx framework and handles relaying transactions. |

## Flow

```mermaid
flowchart TD
    subgraph Node
        A[Reth Node]
    end
    B[SearcherExEx]
    C[PathFinder]
    D[RelayerPool]
    E[ConfigManager]

    A --> B
    B -->|uses| E
    B --> C
    C -->|sends tx| D
    D -->|broadcasts| A
```

The binary starts a Reth node and installs `SearcherExEx`. The extension loads
configuration via `ConfigManager`, finds profitable routes with `PathFinder` and
submits transactions through `RelayerPool`.
