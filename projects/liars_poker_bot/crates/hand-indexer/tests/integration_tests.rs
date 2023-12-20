use std::collections::HashSet;

use hand_indexer::{
    cards::{
        cardset::CardSet,
        iterators::{isomorphic, DealEnumerationIterator, IsomorphicDealIterator},
        Deck,
    },
    HandIndexer,
};

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
        brute_set.insert(d);
    }

    // round 1, flop
    for d in IsomorphicDealIterator::std(deck, &[2, 3]) {
        brute_set.insert(d);
    }

    let mut index_set = HashSet::new();

    let indexer = HandIndexer::poker();
    let h = indexer.unindex(1027).unwrap();
    indexer.index(&h);
    for idx in 0..indexer.index_size(0) + indexer.index_size(1) {
        let deal = indexer
            .unindex(idx)
            .unwrap_or_else(|| panic!("failed to unindex: {}", idx));
        assert_eq!(
            indexer.index(&deal),
            Some(idx),
            "failed to reindex deal: {:?}",
            deal
        );

        // apply the same isomorphism transform so that we can directly compare the hands
        let mut iso_deal = [CardSet::default(); 5];
        deal.into_iter()
            .enumerate()
            .for_each(|(i, x)| iso_deal[i] = x);
        index_set.insert(isomorphic(iso_deal));
    }

    assert_eq!(index_set.len(), brute_set.len());
    assert_eq!(
        index_set.len(),
        indexer.index_size(0) + indexer.index_size(1)
    );
    assert_eq!(index_set, brute_set);
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
