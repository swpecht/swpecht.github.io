use crate::{configurations, Rank};

mod indexer_cache;

mod indexer_state;

const SUITS: usize = 4;
const RANKS: usize = 13;
const CARDS: usize = 52;
const MAX_GROUP_INDEX: usize = 0x1000000;
const ROUND_SHIFT: usize = 4;
const ROUND_MASK: usize = 0xf;

/// Translation of https://github.com/botm/hand-isomorphism
pub struct TranslatedIndexer {
    rounds: usize,
    cards_per_round: Vec<usize>,
    configurations: Vec<usize>,
    permutations: Vec<usize>,
    round_size: Vec<usize>,
    round_start: Vec<usize>,
    permutations_to_configuration: Vec<Vec<usize>>,
    permutations_to_pi: Vec<Vec<usize>>,
    configuration_to_equal: Vec<Vec<usize>>,
    configuration: Vec<Vec<Vec<usize>>>,
    configuration_to_suit_size: Vec<Vec<Vec<usize>>>,
    configuration_to_offset: Vec<Vec<usize>>,
}

impl TranslatedIndexer {
    /// Create a new indexer. This is expensive as it generates lookup tables.
    pub fn new(cards_per_round: &[usize]) -> Self {
        let rounds = cards_per_round.len();

        //     permutationToConfiguration = new int[rounds][];
        //     permutationToPi = new int[rounds][];
        //     configurationToEqual = new int[rounds][];
        //     configuration = new int[rounds][][];
        //     configurationToSuitSize = new int[rounds][][];
        //     configurationToOffset = new long[rounds][];

        assert!(
            cards_per_round.iter().sum::<usize>() <= CARDS,
            "Too many cards"
        );

        let mut round_start = vec![0; rounds];
        let mut j = 0;
        for i in 0..rounds {
            round_start[i] = j;
            j += cards_per_round[i];
        }

        let mut configurations = vec![0; rounds];
        enumerate_configurations(&mut configurations, cards_per_round, false); //count

        // let configu
        //     for (int i = 0; i < rounds; ++i) {
        //       configurationToEqual[i] = new int[configurations[i]];
        //       configurationToOffset[i] = new long[configurations[i]];
        //       configuration[i] = new int[configurations[i]][SUITS];
        //       configurationToSuitSize[i] = new int[configurations[i]][SUITS];
        //     }

        //     configurations = new int[rounds];
        //     enumerateConfigurations(true); //tabulate

        //     roundSize = new long[rounds];
        //     for (int i = 0; i < rounds; ++i) {
        //       long accum = 0;
        //       for (int j = 0; j < configurations[i]; ++j) {
        //         long next = accum + configurationToOffset[i][j];
        //         configurationToOffset[i][j] = accum;
        //         accum = next;
        //       }
        //       roundSize[i] = accum;
        //     }

        //     permutations = new int[rounds];

        //     enumeratePermutations(false); //count

        //     for (int i = 0; i < rounds; ++i) {
        //       permutationToConfiguration[i] = new int[permutations[i]];
        //       permutationToPi[i] = new int[permutations[i]];
        //     }

        //     enumeratePermutations(true); //tabulate
        Self {
            rounds,
            cards_per_round: cards_per_round.to_vec(),
            configurations: todo!(),
            permutations: todo!(),
            round_size: todo!(),
            round_start: todo!(),
            permutations_to_configuration: todo!(),
            permutations_to_pi: todo!(),
            configuration_to_equal: todo!(),
            configuration: todo!(),
            configuration_to_suit_size: todo!(),
            configuration_to_offset: todo!(),
        }
    }
}

// /**
//  * map poker hands to an index shared by all isomorphic hands,
//  * and map an index to a canonical poker hand
//  */
// public class HandIndexer {

//   /**
//    * Index a hand on every round. This is not more expensive than just indexing the last round.
//    *
//    * @param cards
//    * @param indices an array where the indices for every round will be saved to
//    * @return hands index on the last round
//    */
//   public final long indexAll(final int[] cards, final long indices[]) {
//     if (rounds > 0) {
//       HandIndexerState state = new HandIndexerState();
//       for (int i = 0; i < rounds; i++) {
//         indices[i] = indexNextRound(state, cards);
//       }
//       return indices[rounds - 1];
//     }
//     return 0;
//   }

//   /**
//    *  Index a hand on the last round.
//    *
//    * @param cards
//    * @return hand's index on the last round
//    */
//   public final long indexLast(final int... cards) {
//     final long[] indices = new long[rounds];
//     return indexAll(cards, indices);
//   }

//   /**
//    * Incrementally index the next round.
//    *
//    * @param state
//    * @param cards the cards for the next round only!
//    * @return hand's index on the latest round
//    */
//   public final long indexNextRound(final HandIndexerState state, final int[] cards) {
//     int round = state.round++;

//     int[] ranks = new int[SUITS];
//     int[] shiftedRanks = new int[SUITS];

//     for (int i = 0, j = roundStart[round]; i < cardsPerRound[round]; ++i, ++j) {
//       int rank = cards[j] >> 2, suit = cards[j] & 3, rankBit = 1 << rank;
//       ranks[suit] |= rankBit;
//       shiftedRanks[suit] |= rankBit >> Integer.bitCount((rankBit - 1) & state.usedRanks[suit]);
//     }

//     for (int i = 0; i < SUITS; ++i) {
//       int usedSize = Integer.bitCount(state.usedRanks[i]), thisSize = Integer.bitCount(ranks[i]);
//       state.suitIndex[i] += state.suitMultiplier[i] * rankSetToIndex[shiftedRanks[i]];
//       state.suitMultiplier[i] *= nCrRanks[RANKS - usedSize][thisSize];
//       state.usedRanks[i] |= ranks[i];
//     }

//     for (int i = 0, remaining = cardsPerRound[round]; i < SUITS - 1; ++i) {
//       int thisSize = Integer.bitCount(ranks[i]);
//       state.permutationIndex += state.permutationMultiplier * thisSize;
//       state.permutationMultiplier *= remaining + 1;
//       remaining -= thisSize;
//     }

//     int configuration = permutationToConfiguration[round][state.permutationIndex];
//     int piIndex = permutationToPi[round][state.permutationIndex];
//     int equalIndex = configurationToEqual[round][configuration];
//     long offset = configurationToOffset[round][configuration];
//     int[] pi = suitPermutations[piIndex];

//     int[] suitIndex = new int[SUITS], suitMultiplier = new int[SUITS];
//     for (int i = 0; i < SUITS; ++i) {
//       suitIndex[i] = state.suitIndex[pi[i]];
//       suitMultiplier[i] = state.suitMultiplier[pi[i]];
//     }
//     long index = offset, multiplier = 1;
//     for (int i = 0; i < SUITS; ) {
//       long part, size;

//       if (i + 1 < SUITS && equal[equalIndex][i + 1]) {
//         if (i + 2 < SUITS && equal[equalIndex][i + 2]) {
//           if (i + 3 < SUITS && equal[equalIndex][i + 3]) {
//             swap(suitIndex, i, i + 1);
//             swap(suitIndex, i + 2, i + 3);
//             swap(suitIndex, i, i + 2);
//             swap(suitIndex, i + 1, i + 3);
//             swap(suitIndex, i + 1, i + 2);
//             part = suitIndex[i]
//                 + nCrGroups[suitIndex[i + 1] + 1][2]
//                 + nCrGroups[suitIndex[i + 2] + 2][3]
//                 + nCrGroups[suitIndex[i + 3] + 3][4];
//             size = nCrGroups[suitMultiplier[i] + 3][4];
//             i += 4;
//           } else {
//             swap(suitIndex, i, i + 1);
//             swap(suitIndex, i, i + 2);
//             swap(suitIndex, i + 1, i + 2);
//             part = suitIndex[i] + nCrGroups[suitIndex[i + 1] + 1][2]
//                 + nCrGroups[suitIndex[i + 2] + 2][3];
//             size = nCrGroups[suitMultiplier[i] + 2][3];
//             i += 3;
//           }
//         } else {
//           swap(suitIndex, i, i + 1);
//           part = suitIndex[i] + nCrGroups[suitIndex[i + 1] + 1][2];
//           size = nCrGroups[suitMultiplier[i] + 1][2];
//           i += 2;
//         }
//       } else {
//         part = suitIndex[i];
//         size = suitMultiplier[i];
//         i += 1;
//       }

//       index += multiplier * part;
//       multiplier *= size;
//     }
//     return index;
//   }

//   /**
//    * Recover the canonical hand from a particular index.
//    *
//    * @param round
//    * @param index
//    * @param cards
//    * @return true if successful
//    */
//   public boolean unindex(int round, long index, int[] cards) {
//     if (round >= rounds || index >= roundSize[round])
//       return false;

//     int low = 0, high = configurations[round];
//     int configurationIdx = 0;
//     while (Integer.compareUnsigned(low, high) < 0) {
//       int mid = Integer.divideUnsigned(low + high, 2);
//       if (configurationToOffset[round][mid] <= index) {
//         configurationIdx = mid;
//         low = mid + 1;
//       } else {
//         high = mid;
//       }
//     }
//     index -= configurationToOffset[round][configurationIdx];

//     long[] suitIndex = new long[SUITS];
//     for (int i = 0; i < SUITS; ) {
//       int j = i + 1;
//       while (j < SUITS && configuration[round][configurationIdx][j] ==
//           configuration[round][configurationIdx][i]) {
//         ++j;
//       }

//       int suitSize = configurationToSuitSize[round][configurationIdx][i];
//       long groupSize = nCrGroups[suitSize + j - i - 1][j - i];
//       long groupIndex = Long.remainderUnsigned(index, groupSize);
//       index = Long.divideUnsigned(index, groupSize);

//       for (; i < j - 1; ++i) {
//         suitIndex[i] = low = (int) Math.floor(Math.exp(Math.log(groupIndex) / (j - i) - 1
//             + Math.log(j - i)) - j - i);
//         high = (int) Math.ceil(Math.exp(Math.log(groupIndex) / (j - i) + Math.log(j - i)) - j + i
//             + 1);
//         if (Integer.compareUnsigned(high, suitSize) > 0) {
//           high = suitSize;
//         }
//         if (Integer.compareUnsigned(high, low) <= 0) {
//           low = 0;
//         }
//         while (Integer.compareUnsigned(low, high) < 0) {
//           int mid = Integer.divideUnsigned(low + high, 2);
//           if (nCrGroups[mid + j - i - 1][j - i] <= groupIndex) {
//             suitIndex[i] = mid;
//             low = mid + 1;
//           } else {
//             high = mid;
//           }
//         }
//         groupIndex -= nCrGroups[(int) (suitIndex[i] + j - i - 1)][j - i];
//       }

//       suitIndex[i] = groupIndex;
//       ++i;
//     }

//     int[] location = new int[rounds];
//     System.arraycopy(roundStart, 0, location, 0, rounds);
//     for (int i = 0; i < SUITS; ++i) {
//       int used = 0, m = 0;
//       for (int j = 0; j < rounds; ++j) {
//         int n = configuration[round][configurationIdx][i] >> ROUND_SHIFT * (rounds - j - 1)
//             & ROUND_MASK;
//         int roundSize = nCrRanks[RANKS - m][n];
//         m += n;
//         int roundIdx = (int) Long.remainderUnsigned(suitIndex[i], roundSize);
//         suitIndex[i] = Long.divideUnsigned(suitIndex[i], roundSize);
//         int shiftedCards = indexToRankSet[n][roundIdx], rankSet = 0;
//         for (int k = 0; k < n; ++k) {
//           int shiftedCard = shiftedCards & -shiftedCards;
//           shiftedCards ^= shiftedCard;
//           int card = nthUnset[used][Integer.numberOfTrailingZeros(shiftedCard)];
//           rankSet |= 1 << card;
//           cards[location[j]++] = card << 2 | i;
//         }
//         used |= rankSet;
//       }
//     }
//     return true;
//   }

//   private void swap(final int[] suitIndex, final int u, final int v) {
//     if (suitIndex[u] > suitIndex[v]) {
//       int tmp = suitIndex[u];
//       suitIndex[u] = suitIndex[v];
//       suitIndex[v] = tmp;
//     }
//   }

fn enumerate_configurations(
    configurations: &mut Vec<usize>,
    cards_per_round: &[usize],
    tabulate: bool,
) {
    // TODO: can pass in variable to save results later if needed

    let used = [0; SUITS];
    let configuration = [0; SUITS];

    enumerate_configurations_r(
        configurations,
        cards_per_round,
        0,
        cards_per_round[0],
        0,
        (1 << SUITS) - 2,
        used,
        configuration,
        tabulate,
    );
}

fn enumerate_configurations_r<const S: usize>(
    configurations: &mut Vec<usize>,
    cards_per_round: &[usize],
    round: usize,
    remaining: usize,
    suit: usize,
    equal: usize,
    mut used: [usize; S],
    mut configuration: [usize; S],
    tabulate: bool,
) {
    let rounds = cards_per_round.len();
    if suit == S {
        if tabulate {
            tabulate_configurations(round, configuration);
        } else {
            configurations[round] += 1;
        }
        if round + 1 < rounds {
            enumerate_configurations_r(
                configurations,
                cards_per_round,
                round + 1,
                cards_per_round[round + 1],
                0,
                equal,
                used,
                configuration,
                tabulate,
            )
        }
    } else {
        let min = if suit == S - 1 { remaining } else { 0 };
        let mut max = RANKS - used[suit];
        if remaining < max {
            max = remaining;
        }

        let mut previous = (RANKS + 1);
        let was_equal = (equal & 1 << suit) != 0;
        if was_equal {
            previous =
                (configuration[suit - 1] >> (ROUND_SHIFT * (rounds - round - 1))) & ROUND_MASK;
            if (previous) < max {
                max = previous;
            }
        }

        let old_configuration = configuration[suit];
        let old_used = used[suit];

        for i in min..max + 1 {
            let new_configuration = old_configuration | i << (ROUND_SHIFT * (rounds - round - 1));
            let new_equal = (equal & !(1 << suit))
                | ((if was_equal & (i == previous) { 1 } else { 0 }) << suit);
            used[suit] = old_used + i;
            configuration[suit] = new_configuration;
            enumerate_configurations_r(
                configurations,
                cards_per_round,
                round,
                remaining - i,
                suit + 1,
                new_equal,
                used,
                configuration,
                tabulate,
            );
            configuration[suit] = old_configuration;
            used[suit] = old_used;
        }
    }
}

fn tabulate_configurations<const S: usize>(round: usize, configuartion: [usize; S]) {
    //   private void tabulateConfigurations(int round, int[] configuration) {
    //     int id = configurations[round]++;
    //     OUT:
    //     for (; id > 0; --id) {
    //       for (int i = 0; i < SUITS; ++i) {
    //         if (configuration[i] < this.configuration[round][id - 1][i]) {
    //           break;
    //         } else if (configuration[i] > this.configuration[round][id - 1][i]) {
    //           break OUT;
    //         }
    //       }
    //       for (int i = 0; i < SUITS; ++i) {
    //         this.configuration[round][id][i] = this.configuration[round][id - 1][i];
    //         configurationToSuitSize[round][id][i] = configurationToSuitSize[round][id - 1][i];
    //       }
    //       configurationToOffset[round][id] = configurationToOffset[round][id - 1];
    //       configurationToEqual[round][id] = configurationToEqual[round][id - 1];
    //     }

    //     configurationToOffset[round][id] = 1;
    //     System.arraycopy(configuration, 0, this.configuration[round][id], 0, SUITS);

    //     int equal = 0;
    //     for (int i = 0; i < SUITS; ) {
    //       int size = 1;
    //       for (int j = 0, remaining = RANKS; j <= round; ++j) {
    //         int ranks = configuration[i] >> ROUND_SHIFT * (rounds - j - 1) & ROUND_MASK;
    //         size *= nCrRanks[remaining][ranks];
    //         remaining -= ranks;
    //       }

    //       int j = i + 1;
    //       while (j < SUITS && configuration[j] == configuration[i]) {
    //         ++j;
    //       }

    //       for (int k = i; k < j; ++k) {
    //         configurationToSuitSize[round][id][k] = size;
    //       }

    //       configurationToOffset[round][id] *= nCrGroups[size + j - i - 1][j - i];

    //       for (int k = i + 1; k < j; ++k) {
    //         equal |= 1 << k;
    //       }

    //       i = j;
    //     }

    //     configurationToEqual[round][id] = equal >> 1;
    //   }
    todo!()
}

//   private void enumeratePermutations(boolean tabulate) {
//     int[] used = new int[SUITS];
//     int[] count = new int[SUITS];

//     enumeratePermutationsR(0, cardsPerRound[0], 0, used, count, tabulate);
//   }

//   private void enumeratePermutationsR(int round, int remaining, int suit, int[] used, int[]
//       count, boolean tabulate) {
//     if (suit == SUITS) {
//       if (tabulate) {
//         tabulatePermutations(round, count);
//       } else {
//         countPermutations(round, count);
//       }

//       if (round + 1 < rounds) {
//         enumeratePermutationsR(round + 1, cardsPerRound[round + 1], 0, used, count, tabulate);
//       }
//     } else {
//       int min = 0;
//       if (suit == SUITS - 1) {
//         min = remaining;
//       }

//       int max = RANKS - used[suit];
//       if (remaining < max) {
//         max = remaining;
//       }

//       int oldCount = count[suit], oldUsed = used[suit];
//       for (int i = min; i <= max; ++i) {
//         int newCount = oldCount | i << ROUND_SHIFT * (rounds - round - 1);

//         used[suit] = oldUsed + i;
//         count[suit] = newCount;
//         enumeratePermutationsR(round, remaining - i, suit + 1, used, count, tabulate);
//         count[suit] = oldCount;
//         used[suit] = oldUsed;
//       }
//     }
//   }

//   private void countPermutations(int round, int count[]) {
//     int idx = 0, mult = 1;
//     for (int i = 0; i <= round; ++i) {
//       for (int j = 0, remaining = cardsPerRound[i]; j < SUITS - 1; ++j) {
//         int size = count[j] >> (rounds - i - 1) * ROUND_SHIFT & ROUND_MASK;
//         idx += mult * size;
//         mult *= remaining + 1;
//         remaining -= size;
//       }
//     }

//     if (permutations[round] < idx + 1) {
//       permutations[round] = idx + 1;
//     }
//   }

//   private void tabulatePermutations(int round, int[] count) {
//     int idx = 0, mult = 1;
//     for (int i = 0; i <= round; ++i) {
//       for (int j = 0, remaining = cardsPerRound[i]; j < SUITS - 1; ++j) {
//         int size = count[j] >> (rounds - i - 1) * ROUND_SHIFT & ROUND_MASK;
//         idx += mult * size;
//         mult *= remaining + 1;
//         remaining -= size;
//       }
//     }

//     int[] pi = new int[SUITS];
//     for (int i = 0; i < SUITS; ++i) {
//       pi[i] = i;
//     }

//     for (int i = 1; i < SUITS; ++i) {
//       int j = i, pi_i = pi[i];
//       for (; j > 0; --j) {
//         if (count[pi_i] > count[pi[j - 1]]) {
//           pi[j] = pi[j - 1];
//         } else {
//           break;
//         }
//       }
//       pi[j] = pi_i;
//     }

//     int pi_idx = 0, pi_mult = 1, pi_used = 0;
//     for (int i = 0; i < SUITS; ++i) {
//       int this_bit = 1 << pi[i];
//       int smaller = Integer.bitCount((this_bit - 1) & pi_used);
//       pi_idx += (pi[i] - smaller) * pi_mult;
//       pi_mult *= SUITS - i;
//       pi_used |= this_bit;
//     }

//     permutationToPi[round][idx] = pi_idx;

//     int low = 0, high = configurations[round];
//     while (low < high) {
//       int mid = (low + high) / 2;

//       int compare = 0;
//       for (int i = 0; i < SUITS; ++i) {
//         int that = count[pi[i]];
//         int other = configuration[round][mid][i];
//         if (other > that) {
//           compare = -1;
//           break;
//         } else if (other < that) {
//           compare = 1;
//           break;
//         }
//       }

//       if (compare == -1) {
//         high = mid;
//       } else if (compare == 0) {
//         low = high = mid;
//       } else {
//         low = mid + 1;
//       }
//     }

//     permutationToConfiguration[round][idx] = low;
//   }
// }

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_construct_indexer() {
        let index = TranslatedIndexer::new(&[2, 3]);
        todo!()
    }
}
