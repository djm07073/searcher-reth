use alloy_primitives::Address;

use itertools::{ Either, Itertools };
use searcher_reth_repository::types::Priority;

use super::types::{Hop, RoutePath};

// A -> B -> A
// A -> B -> C -> A
pub fn get_candidates(
    dexs: Vec<(Address, u8)>,
    tokens: Vec<(Address, Priority)>
) -> Vec<RoutePath> {
    let mut route_paths = Vec::new();

    let (beginning_tokens, other_tokens): (Vec<Address>, Vec<Address>) = tokens
        .iter()
        .partition_map(|(addr, p)| {
            if *p == Priority::Beginning { Either::Left(*addr) } else { Either::Right(*addr) }
        });

    // Case 1: A -> B -> A (2-hop paths)
    for start_token in &beginning_tokens {
        for inter_token in &other_tokens {
            for dex_hops in dexs.iter().permutations(2) {
                let hops = vec![
                    // A -> B
                    Hop {
                        dexType: dex_hops[0].1,
                        dex: dex_hops[0].0,
                        srcToken: *start_token,
                        dstToken: *inter_token,
                    },
                    // B -> A
                    Hop {
                        dexType: dex_hops[1].1,
                        dex: dex_hops[1].0,
                        srcToken: *inter_token,
                        dstToken: *start_token,
                    }
                ];
                route_paths.push(RoutePath {
                    hops,
                });
            }
        }
    }

    // Case 2: A -> B -> C -> A (3-hop paths)
    for start_token in &beginning_tokens {
        for inter_token_pair in other_tokens.iter().combinations(2) {
            for dex_hops in dexs.iter().permutations(3) {
                let hops = vec![
                    // A -> B
                    Hop {
                        dexType: dex_hops[0].1,
                        dex: dex_hops[0].0,
                        srcToken: *start_token,
                        dstToken: *inter_token_pair[0],
                    },
                    // B -> C
                    Hop {
                        dexType: dex_hops[1].1,
                        dex: dex_hops[1].0,
                        srcToken: *inter_token_pair[0],
                        dstToken: *inter_token_pair[1],
                    },
                    // C -> A
                    Hop {
                        dexType: dex_hops[2].1,
                        dex: dex_hops[2].0,
                        srcToken: *inter_token_pair[1],
                        dstToken: *start_token,
                    }
                ];

                route_paths.push(RoutePath {
                    hops,
                });
            }
        }
    }

    route_paths
}
