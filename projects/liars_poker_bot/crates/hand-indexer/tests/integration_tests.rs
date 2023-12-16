use hand_indexer::{
    cards::{
        iterators::{DealEnumerationIterator, IsomorphicDealIterator},
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
    assert_eq!(IsomorphicDealIterator::new(deck, &[2]).count(), 169);
    assert_eq!(
        IsomorphicDealIterator::new(deck, &[2, 3]).count(),
        1_286_792
    );
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
    assert_eq!(indexer.max_index(0), 169);
    assert_eq!(indexer.max_index(1), 1_286_792); // from isomorphism paper
    assert_eq!(indexer.max_index(2), 55_190_538); // from isomorphism paper

    // Not yet supported for performance reasons
    // assert_eq!(indexer.max_index(3), 2_428_287_420); // from isomorphism paper
}
