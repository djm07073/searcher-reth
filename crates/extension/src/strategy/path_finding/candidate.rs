use std::collections::HashMap;

use alloy_primitives::Address;

use itertools::{Either, Itertools};
use searcher_reth_repository::types::Priority;

use super::types::{Hop, RoutePath};

// A -> B -> A
// A -> B -> C -> A
pub fn get_candidates(
    dexs: Vec<(Address, u8)>,
    tokens: Vec<(Address, Priority)>,
) -> Vec<HashMap<Address, Vec<RoutePath>>> {
    let (beginning_tokens, other_tokens): (Vec<Address>, Vec<Address>) =
        tokens.iter().partition_map(|(addr, p)| {
            if *p == Priority::Beginning { Either::Left(*addr) } else { Either::Right(*addr) }
        });

    let mut hop2 = HashMap::new();
    let mut hop3 = HashMap::new();

    for start_token in &beginning_tokens {
        let mut hop2_paths = Vec::new();

        // Case 1: A -> B -> A (2-hop paths)
        for inter_token in &other_tokens {
            for dex_hops in dexs.iter().permutations(2) {
                let path = vec![
                    Hop {
                        dexType: dex_hops[0].1,
                        dex: dex_hops[0].0,
                        srcToken: *start_token,
                        dstToken: *inter_token,
                    },
                    Hop {
                        dexType: dex_hops[1].1,
                        dex: dex_hops[1].0,
                        srcToken: *inter_token,
                        dstToken: *start_token,
                    },
                ];
                hop2_paths.push(path);
            }
        }
        hop2.insert(*start_token, hop2_paths);

        let mut hop3_paths = Vec::new();
        // Case 2: A -> B -> C -> A (3-hop paths)
        for inter_token_pair in other_tokens.iter().combinations(2) {
            for dex_hops in dexs.iter().permutations(3) {
                let path = vec![
                    Hop {
                        dexType: dex_hops[0].1,
                        dex: dex_hops[0].0,
                        srcToken: *start_token,
                        dstToken: *inter_token_pair[0],
                    },
                    Hop {
                        dexType: dex_hops[1].1,
                        dex: dex_hops[1].0,
                        srcToken: *inter_token_pair[0],
                        dstToken: *inter_token_pair[1],
                    },
                    Hop {
                        dexType: dex_hops[2].1,
                        dex: dex_hops[2].0,
                        srcToken: *inter_token_pair[1],
                        dstToken: *start_token,
                    },
                ];
                hop3_paths.push(path);
            }
        }

        hop3.insert(*start_token, hop3_paths);
    }

    vec![hop2, hop3]
}
