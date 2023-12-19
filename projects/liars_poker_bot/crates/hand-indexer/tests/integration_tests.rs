use std::collections::HashSet;

use hand_indexer::{
    cards::{
        cardset::CardSet,
        iterators::{DealEnumerationIterator, IsomorphicDealIterator},
        Card, Deck,
    },
    HandIndexer,
};
use itertools::Itertools;

#[test]
fn test_count_deals() {
    // Flop: 52 choose 2 * 50 choose 3
    assert_eq!(count_combinations([2, 3]), 25_989_600);
    // Turn: 52 choose 2 * 50 choose 3 * 47
    assert_eq!(count_combinations([2, 3, 1]), 1_221_511_200);
    // // River: 52 choose 2 * 50 choose 3 * 47 * 46
    // assert_eq!(count_combinations([2, 3, 1, 1]), 56_189_515_200);

    let deck = Deck::standard();
    assert_eq!(IsomorphicDealIterator::std(deck, &[2]).count(), 169);
    assert_eq!(
        IsomorphicDealIterator::std(deck, &[2, 3]).count(),
        1_286_792
    );
}

#[test]
fn test_poker_indexer() {
    let deck = Deck::standard();
    let mut brute_set = HashSet::new();

    // round 0, pocket
    for d in IsomorphicDealIterator::std(deck, &[2]) {
        let deal = d
            .into_iter()
            .filter(|&x| x != CardSet::default())
            .collect_vec();
        brute_set.insert(deal);
    }

    // // round 1, flop
    // for d in IsomorphicDealIterator::std(deck, &[2, 3]) {
    //     let deal = d
    //         .into_iter()
    //         .filter(|&x| x != CardSet::default())
    //         .collect_vec();
    //     brute_set.insert(deal);
    // }

    let mut index_set = HashSet::new();

    let indexer = HandIndexer::poker();
    let h = indexer.unindex(0).unwrap();
    indexer.index(&h);
    for idx in 0..indexer.index_size(0) {
        let deal = indexer.unindex(idx).unwrap();
        assert!(deal.iter().all(|&x| x != CardSet::default()));
        assert!(!deal.is_empty(), "{} is empty", idx);
        assert_eq!(
            indexer.index(&deal),
            Some(idx),
            "failed to reindex deal: {:?}",
            deal
        );
        assert_eq!(deal.len(), 1, "{}: {:?}", idx, deal);
        index_set.insert(deal);
    }

    assert_eq!(index_set.len(), brute_set.len());
    // assert_eq!(index_set, brute_set);

    for deal in index_set {
        assert!(brute_set.contains(&deal), "{:?}", deal);
    }
    // 10000 0000000000001

    // let mut index_list = index_set.into_iter().collect_vec();
    // index_list.sort();

    // let mut brute_list = brute_set.into_iter().collect_vec();
    // brute_list.sort();

    // // assert_eq!(index_list, brute_list);
    // let mut diff = Vec::new();
    // for i in 0..index_list.len() {
    //     if index_list[i] != brute_list[i] {
    //         diff.push((i, index_list[i].clone(), brute_list[i].clone()));
    //     }
    // }
    // assert!(diff.is_empty(), "{:#?}", diff);
}

fn count_combinations<const R: usize>(cards_per_round: [usize; R]) -> usize {
    let deck = Deck::standard();
    let mut count = 0;

    for _ in DealEnumerationIterator::new(deck, &cards_per_round) {
        count += 1;
    }

    count
}

#[test]
fn test_poker_index_size() {
    let indexer = HandIndexer::poker();
    assert_eq!(indexer.index_size(0), 169);
    assert_eq!(indexer.index_size(1), 1_286_792); // from isomorphism paper

    // Not yet supported for performance reasons
    // assert_eq!(indexer.max_index(2), 169 + 1_286_792 + 55_190_538); // from isomorphism paper
    // assert_eq!(indexer.max_index(3), 2_428_287_420); // from isomorphism paper
}
